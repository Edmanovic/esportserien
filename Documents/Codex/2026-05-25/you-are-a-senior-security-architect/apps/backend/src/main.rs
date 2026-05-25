//! Minimal ESPASS encrypted sync API prototype.
//!
//! This server stores encrypted payloads, device identities, and session
//! metadata only. It has no endpoints that accept master passwords, plaintext
//! credentials, plaintext TOTP seeds, or vault keys.

mod security;
mod rate_limit;
mod anomaly;

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, RwLock};

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
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            encrypted_items: Arc::new(RwLock::new(BTreeMap::new())),
            devices: Arc::new(RwLock::new(BTreeMap::new())),
            rate_limiter: Arc::new(RwLock::new(RateLimiter::new(120, 60))),
            replay: Arc::new(RwLock::new(RequestReplayProtector::default())),
            anomaly: Arc::new(RwLock::new(AnomalyDetector::new(60, 200))),
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
    devices.insert(registration.identity.device_id, registration.identity);
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
    items.insert((vault_id, item_id), item);
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
    Ok(Json(
        items
            .get(&(vault_id, item_id))
            .map(|item| item.encrypted_payload.clone()),
    ))
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
