//! Authentication handlers: register, login, and refresh.

use argon2::password_hash::{rand_core::OsRng, PasswordHasher, SaltString};
use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{db, AppState};

// ── JWT ──────────────────────────────────────────────────────────────────────

const JWT_LIFETIME_SECS: i64 = 900;       // 15 minutes
const REFRESH_LIFETIME_SECS: i64 = 604_800; // 7 days

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,      // user_id
    pub vault_id: String,
    pub exp: usize,
    pub iat: usize,
}

fn jwt_secret() -> String {
    let secret = std::env::var("ESPASS_JWT_SECRET")
        .unwrap_or_else(|_| "dev-secret-change-in-production".to_string());
    #[cfg(not(debug_assertions))]
    if secret.len() < 32 {
        panic!("ESPASS_JWT_SECRET must be at least 32 characters in production builds");
    }
    secret
}

pub fn sign_jwt(user_id: &str, vault_id: &str) -> Result<String, StatusCode> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let claims = Claims {
        sub: user_id.to_string(),
        vault_id: vault_id.to_string(),
        exp: (now + JWT_LIFETIME_SECS) as usize,
        iat: now as usize,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret().as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
}

pub fn verify_jwt(token: &str) -> Result<Claims, ()> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_secret().as_bytes()),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|_| ())
}

fn new_refresh_token() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::thread_rng().gen();
    hex::encode(bytes)
}

fn hash_refresh_token(token: &str) -> String {
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    hex::encode(hasher.finalize())
}

// ── Request / response types ─────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub auth_secret_hex: String,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub vault_id: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub auth_secret_hex: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub jwt: String,
    pub refresh_token: String,
    pub user_id: String,
    pub vault_id: String,
}

#[derive(Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

// ── Handlers ─────────────────────────────────────────────────────────────────

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, StatusCode> {
    let secret_bytes =
        hex::decode(&req.auth_secret_hex).map_err(|_| StatusCode::BAD_REQUEST)?;

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(&secret_bytes, &salt)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .to_string();

    let user_id = Uuid::new_v4().to_string();
    let vault_id = Uuid::new_v4().to_string();

    let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    db::create_user(&db, &user_id, &req.email, &hash, &vault_id)
        .map_err(|_| StatusCode::CONFLICT)?; // UNIQUE constraint on email

    Ok(Json(RegisterResponse { user_id, vault_id }))
}

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let secret_bytes =
        hex::decode(&req.auth_secret_hex).map_err(|_| StatusCode::BAD_REQUEST)?;

    let (user_id, vault_id, auth_hash) = {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let user = db::load_user_by_email(&db, &req.email)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;
        (user.id, user.vault_id, user.auth_hash)
    };

    let parsed = PasswordHash::new(&auth_hash).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Argon2::default()
        .verify_password(&secret_bytes, &parsed)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let jwt = sign_jwt(&user_id, &vault_id)?;
    let refresh_token = new_refresh_token();
    let expires_at =
        time::OffsetDateTime::now_utc().unix_timestamp() + REFRESH_LIFETIME_SECS;

    {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db::store_refresh_token(&db, &hash_refresh_token(&refresh_token), &user_id, expires_at)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(LoginResponse { jwt, refresh_token, user_id, vault_id }))
}

pub async fn refresh_token(
    State(state): State<AppState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<LoginResponse>, StatusCode> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    let (user_id, vault_id) = {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Validate old token
        let uid = db::validate_refresh_token(&db, &hash_refresh_token(&req.refresh_token), now)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?;

        // Delete old token
        db::delete_refresh_token(&db, &hash_refresh_token(&req.refresh_token))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        // Get vault_id for the user
        let mut stmt = db.prepare("SELECT vault_id FROM users WHERE id = ?1")
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut rows = stmt.query(rusqlite::params![uid])
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let vid = rows.next()
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::UNAUTHORIZED)?
            .get::<_, String>(0)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

        (uid, vid)
    };

    let jwt = sign_jwt(&user_id, &vault_id)?;
    let new_token = new_refresh_token();
    let expires_at = now + REFRESH_LIFETIME_SECS;

    {
        let db = state.db.lock().map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        db::store_refresh_token(&db, &hash_refresh_token(&new_token), &user_id, expires_at)
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    Ok(Json(LoginResponse {
        jwt,
        refresh_token: new_token,
        user_id,
        vault_id,
    }))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_round_trip() {
        let token = sign_jwt("user-123", "vault-456").unwrap();
        let claims = verify_jwt(&token).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.vault_id, "vault-456");
    }

    #[test]
    fn invalid_jwt_rejected() {
        assert!(verify_jwt("not.a.jwt").is_err());
    }
}
