//! SQLite persistence for the ESPASS sync backend.
//!
//! The server is zero-knowledge: it stores ciphertext blobs and device
//! public keys only. It never receives or stores vault keys or plaintext.

use rusqlite::{Connection, Result as SqlResult, params};
use std::path::Path;

/// Opens (or creates) the SQLite database at the given path.
pub fn open(path: &Path) -> SqlResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
    create_schema(&conn)?;
    Ok(conn)
}

fn create_schema(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS vault_items (
            vault_id     TEXT NOT NULL,
            item_id      TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            stored_at    INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY (vault_id, item_id)
        );

        CREATE TABLE IF NOT EXISTS devices (
            device_id     TEXT PRIMARY KEY,
            identity_json TEXT NOT NULL,
            registered_at INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS users (
            id         TEXT PRIMARY KEY,
            email      TEXT UNIQUE NOT NULL,
            auth_hash  TEXT NOT NULL,
            vault_id   TEXT NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE TABLE IF NOT EXISTS refresh_tokens (
            token      TEXT PRIMARY KEY,
            user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            expires_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL DEFAULT (unixepoch())
        );

        CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
    ",
    )
}

/// Upserts an encrypted vault item. `payload_json` is a JSON-serialized `VaultItem`.
pub fn upsert_item(
    conn: &Connection,
    vault_id: &str,
    item_id: &str,
    payload_json: &str,
) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO vault_items (vault_id, item_id, payload_json)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(vault_id, item_id) DO UPDATE SET payload_json = excluded.payload_json, stored_at = unixepoch()",
        params![vault_id, item_id, payload_json],
    )?;
    Ok(())
}

/// Loads a vault item by (vault_id, item_id). Returns None if not found.
pub fn load_item(
    conn: &Connection,
    vault_id: &str,
    item_id: &str,
) -> SqlResult<Option<String>> {
    let mut stmt =
        conn.prepare("SELECT payload_json FROM vault_items WHERE vault_id = ?1 AND item_id = ?2")?;
    let mut rows = stmt.query(params![vault_id, item_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

/// Upserts a device identity. `identity_json` is a JSON-serialized `DeviceIdentity`.
pub fn upsert_device(
    conn: &Connection,
    device_id: &str,
    identity_json: &str,
) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO devices (device_id, identity_json)
         VALUES (?1, ?2)
         ON CONFLICT(device_id) DO UPDATE SET identity_json = excluded.identity_json",
        params![device_id, identity_json],
    )?;
    Ok(())
}

/// Loads a device identity by device_id. Returns None if not found.
pub fn load_device(conn: &Connection, device_id: &str) -> SqlResult<Option<String>> {
    let mut stmt =
        conn.prepare("SELECT identity_json FROM devices WHERE device_id = ?1")?;
    let mut rows = stmt.query(params![device_id])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

/// Row returned from the users table.
pub struct UserRow {
    pub id: String,
    pub email: String,
    pub auth_hash: String,
    pub vault_id: String,
}

/// Creates a new user. `auth_hash` must be an Argon2id PHC string (e.g. from `argon2::PasswordHasher::hash_password`).
/// Returns `Err` if the email is already taken (SQLITE_CONSTRAINT).
pub fn create_user(
    conn: &Connection,
    id: &str,
    email: &str,
    auth_hash: &str,
    vault_id: &str,
) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO users (id, email, auth_hash, vault_id) VALUES (?1, ?2, ?3, ?4)",
        params![id, email, auth_hash, vault_id],
    )?;
    Ok(())
}

/// Loads a user by email. Returns `None` if not found.
pub fn load_user_by_email(conn: &Connection, email: &str) -> SqlResult<Option<UserRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, email, auth_hash, vault_id FROM users WHERE email = ?1",
    )?;
    let mut rows = stmt.query(params![email])?;
    if let Some(row) = rows.next()? {
        Ok(Some(UserRow {
            id: row.get(0)?,
            email: row.get(1)?,
            auth_hash: row.get(2)?,
            vault_id: row.get(3)?,
        }))
    } else {
        Ok(None)
    }
}

/// Stores a refresh token. `expires_at` is a Unix timestamp.
pub fn store_refresh_token(
    conn: &Connection,
    token: &str,
    user_id: &str,
    expires_at: i64,
) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO refresh_tokens (token, user_id, expires_at) VALUES (?1, ?2, ?3)",
        params![token, user_id, expires_at],
    )?;
    Ok(())
}

/// Validates a refresh token and returns the associated user_id.
/// Returns `None` if the token doesn't exist or is expired.
pub fn validate_refresh_token(
    conn: &Connection,
    token: &str,
    now: i64,
) -> SqlResult<Option<String>> {
    let mut stmt = conn.prepare(
        "SELECT user_id FROM refresh_tokens WHERE token = ?1 AND expires_at > ?2",
    )?;
    let mut rows = stmt.query(params![token, now])?;
    if let Some(row) = rows.next()? {
        Ok(Some(row.get(0)?))
    } else {
        Ok(None)
    }
}

/// Deletes a refresh token (used on refresh to rotate it).
pub fn delete_refresh_token(conn: &Connection, token: &str) -> SqlResult<()> {
    let changed = conn.execute("DELETE FROM refresh_tokens WHERE token = ?1", params![token])?;
    if changed == 0 {
        return Err(rusqlite::Error::QueryReturnedNoRows);
    }
    Ok(())
}

/// Loads all item payloads for a vault (for the list metadata endpoint).
pub fn load_items_for_vault(conn: &Connection, vault_id: &str) -> SqlResult<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT payload_json FROM vault_items WHERE vault_id = ?1")?;
    let rows = stmt.query_map(params![vault_id], |row| row.get(0))?;
    rows.collect()
}
