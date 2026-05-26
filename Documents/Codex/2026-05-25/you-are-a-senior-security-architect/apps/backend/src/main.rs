//! Minimal ESPASS encrypted sync API prototype.
//!
//! This server stores encrypted payloads, device identities, and session
//! metadata only. It has no endpoints that accept master passwords, plaintext
//! credentials, plaintext TOTP seeds, or vault keys.

mod security;
mod rate_limit;
mod anomaly;
mod db;
mod auth;

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex, RwLock};

use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::routing::{get, post, put};
use axum::{Json, Router};
use auth::Claims;
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

/// Axum extractor that validates the Bearer JWT and extracts claims.
pub struct BearerAuth(pub Claims);

#[async_trait::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for BearerAuth {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .ok_or(StatusCode::UNAUTHORIZED)?;

        auth::verify_jwt(header)
            .map(BearerAuth)
            .map_err(|_| StatusCode::UNAUTHORIZED)
    }
}

#[derive(serde::Serialize)]
struct VaultItemMeta {
    item_id: Uuid,
    updated_at: i64,
    revision: u64,
    deleted_at: Option<i64>,
}

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let state = AppState::default();
    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/auth/register", post(auth::register))
        .route("/v1/auth/login", post(auth::login))
        .route("/v1/auth/refresh", post(auth::refresh_token))
        .route("/v1/devices", put(register_device))
        .route(
            "/v1/sessions/:device_id/replay/:counter",
            post(validate_replay_counter),
        )
        .route(
            "/v1/vaults/:vault_id/items",
            get(list_vault_items),
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
    BearerAuth(claims): BearerAuth,
    State(state): State<AppState>,
    Path((vault_id, item_id)): Path<(Uuid, Uuid)>,
    Json(item): Json<VaultItem>,
) -> Result<StatusCode, StatusCode> {
    if claims.vault_id != vault_id.to_string() {
        return Err(StatusCode::FORBIDDEN);
    }
    if item.vault_id != vault_id || item.item_id != item_id {
        return Err(StatusCode::BAD_REQUEST);
    }
    reject_suspicious_payload(&item.encrypted_payload)?;
    enforce_local_rate_limit(&state)?;

    // Conflict detection + revision assignment
    let new_revision = {
        let items = state
            .encrypted_items
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        if let Some(existing) = items.get(&(vault_id, item_id)) {
            if let Some(base) = item.base_revision {
                if existing.revision > base {
                    return Err(StatusCode::CONFLICT);
                }
            }
            existing.revision + 1
        } else {
            1
        }
    };

    let mut stored = item;
    stored.revision = new_revision;

    {
        let mut items = state
            .encrypted_items
            .write()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        items.insert((vault_id, item_id), stored.clone());
    }

    {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let json = serde_json::to_string(&stored)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db::upsert_item(&db, &vault_id.to_string(), &item_id.to_string(), &json)
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
    BearerAuth(claims): BearerAuth,
    State(state): State<AppState>,
    Path((vault_id, item_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<Option<EncryptedPayload>>, StatusCode> {
    if claims.vault_id != vault_id.to_string() {
        return Err(StatusCode::FORBIDDEN);
    }
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

async fn list_vault_items(
    BearerAuth(claims): BearerAuth,
    State(state): State<AppState>,
    axum::extract::Path(vault_id): axum::extract::Path<Uuid>,
) -> Result<Json<Vec<VaultItemMeta>>, StatusCode> {
    if claims.vault_id != vault_id.to_string() {
        return Err(StatusCode::FORBIDDEN);
    }

    // In-memory pass first
    let in_memory: Vec<VaultItemMeta> = {
        let items = state
            .encrypted_items
            .read()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        items
            .iter()
            .filter(|((vid, _), _)| *vid == vault_id)
            .map(|(_, item)| VaultItemMeta {
                item_id: item.item_id,
                updated_at: item.updated_at.unix_timestamp(),
                revision: item.revision,
                deleted_at: item.deleted_at.map(|dt| dt.unix_timestamp()),
            })
            .collect()
    };

    if !in_memory.is_empty() {
        return Ok(Json(in_memory));
    }

    // Fall back to SQLite
    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let jsons = db::load_items_for_vault(&db, &vault_id.to_string())
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let metas = jsons
        .iter()
        .filter_map(|j| {
            let item: VaultItem = serde_json::from_str(j).ok()?;
            Some(VaultItemMeta {
                item_id: item.item_id,
                updated_at: item.updated_at.unix_timestamp(),
                revision: item.revision,
                deleted_at: item.deleted_at.map(|dt| dt.unix_timestamp()),
            })
        })
        .collect();

    Ok(Json(metas))
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
