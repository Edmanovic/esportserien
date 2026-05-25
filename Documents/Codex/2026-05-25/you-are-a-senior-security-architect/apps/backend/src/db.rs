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
            vault_id    TEXT NOT NULL,
            item_id     TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            stored_at   INTEGER NOT NULL DEFAULT (unixepoch()),
            PRIMARY KEY (vault_id, item_id)
        );

        CREATE TABLE IF NOT EXISTS devices (
            device_id   TEXT PRIMARY KEY,
            identity_json TEXT NOT NULL,
            registered_at INTEGER NOT NULL DEFAULT (unixepoch())
        );
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
