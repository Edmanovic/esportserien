//! Minimal ESPASS encrypted sync API prototype.
//!
//! This server stores encrypted payloads, device identities, and session
//! metadata only. It has no endpoints that accept master passwords, plaintext
//! credentials, plaintext TOTP seeds, or vault keys.

mod security;
mod rate_limit;
mod anomaly;
mod db;

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex, RwLock};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use espass_shared_types::device::{DeviceIdentity, DeviceRegistration};
use espass_shared_types::vault::{EncryptedPayload, VaultItem};
use security::{RateLimiter, RequestReplayProtector, UploadIntegrityVerifier};
use anomaly::AnomalyDetector;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    encrypted_items: Arc<RwLock<BTreeMap<(Uuid, Uuid), VaultItem>>>,
    devices: Arc<RwLock<BTreeMap<Uuid, DeviceIdentity>>>,
    rate_limiter: Arc<RwLock<RateLimiter>>,
    replay: Arc<RwLock<RequestReplayProtector>>,
    anomaly: Arc<RwLock<AnomalyDetector>>,
    db: Arc<Mutex<rusqlite::Connection>>,
}

impl Default for AppState {
    fn default() -> Self {
        let db_path = std::env::var("ESPASS_DB_PATH")
            .unwrap_or_else(|_| "espass_backend.sqlite".to_string());
        let conn = db::open(std::path::Path::new(&db_path))
            .expect("failed to open SQLite database");

        Self {
            encrypted_items: Arc::new(RwLock::new(BTreeMap::new())),
            devices: Arc::new(RwLock::new(BTreeMap::new())),
            rate_limiter: Arc::new(RwLock::new(RateLimiter::new(120, 60))),
            replay: Arc::new(RwLock::new(RequestReplayProtector::default())),
            anomaly: Arc::new(RwLock::new(AnomalyDetector::new(60, 200))),
            db: Arc::new(Mutex::new(conn)),
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let state = AppState::default();
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/devices", put(register_device))
        .route(
            "/v1/sessions/:device_id/replay/:counter",
            post(validate_replay_counter),
        )
        .route(
            "/v1/vaults/:vault_id/items/:item_id",
            put(upload_item).get(download_item),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:8787").await?;
    axum::serve(listener, app).await
}

async fn healthz() -> &'static str {
    "ok"
}

async fn register_device(
    State(state): State<AppState>,
    Json(registration): Json<DeviceRegistration>,
) -> Result<StatusCode, StatusCode> {
    registration
        .verify()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    let mut devices = state
        .devices
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    devices.insert(registration.identity.device_id, registration.identity.clone());

    // Persist to SQLite
    {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let device_id_str = registration.identity.device_id.to_string();
        let identity_json = serde_json::to_string(&registration.identity)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db::upsert_device(&db, &device_id_str, &identity_json)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(StatusCode::CREATED)
}

async fn upload_item(
    State(state): State<AppState>,
    Path((vault_id, item_id)): Path<(Uuid, Uuid)>,
    Json(item): Json<VaultItem>,
) -> Result<StatusCode, StatusCode> {
    enforce_local_rate_limit(&state)?;
    if item.vault_id != vault_id || item.item_id != item_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    reject_suspicious_payload(&item.encrypted_payload)?;
    let mut items = state
        .encrypted_items
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    items.insert((vault_id, item_id), item.clone());

    // Persist to SQLite
    {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let vault_id_str = vault_id.to_string();
        let item_id_str = item_id.to_string();
        let payload_json = serde_json::to_string(&item)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db::upsert_item(&db, &vault_id_str, &item_id_str, &payload_json)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn validate_replay_counter(
    State(state): State<AppState>,
    Path((device_id, counter)): Path<(Uuid, u64)>,
) -> Result<StatusCode, StatusCode> {
    enforce_local_rate_limit(&state)?;
    let mut replay = state
        .replay
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    replay
        .validate(device_id, counter)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn download_item(
    State(state): State<AppState>,
    Path((vault_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Option<EncryptedPayload>>, StatusCode> {
    let items = state
        .encrypted_items
        .read()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    // Try in-memory first
    if let Some(item) = items.get(&(vault_id, item_id)) {
        return Ok(Json(Some(item.encrypted_payload.clone())));
    }

    drop(items); // Release read lock

    // Fall back to SQLite
    {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let vault_id_str = vault_id.to_string();
        let item_id_str = item_id.to_string();
        if let Some(payload_json) = db::load_item(&db, &vault_id_str, &item_id_str)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        {
            let item: VaultItem = serde_json::from_str(&payload_json)
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
            return Ok(Json(Some(item.encrypted_payload)));
        }
    }

    Ok(Json(None))
}

fn reject_suspicious_payload(payload: &EncryptedPayload) -> Result<(), StatusCode> {
    UploadIntegrityVerifier::validate(payload).map_err(|_| StatusCode::BAD_REQUEST)
}

fn enforce_local_rate_limit(state: &AppState) -> Result<(), StatusCode> {
    let mut limiter = state
        .rate_limiter
        .write()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    limiter
        .check(
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            time::OffsetDateTime::now_utc(),
        )
        .map_err(|_| StatusCode::TOO_MANY_REQUESTS)
}
