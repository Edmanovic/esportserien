# ESPASS Desktop Sync Design

## Goal

Add zero-knowledge cloud sync to the ESPASS desktop app so credentials stay
current across multiple devices. The backend stores encrypted blobs only and
never receives master passwords, vault keys, or plaintext credentials.

## Architecture

Per-item sync over HTTPS. Each credential is encrypted individually with the
vault key and pushed to or pulled from the backend as a `VaultItem`. Conflict
resolution is last-write-wins based on `updated_at` timestamps. Auth uses a
short-lived JWT derived from the master password — nothing auth-related is
ever written to disk.

**Tech stack additions:** `reqwest` (TLS HTTP client) + `jsonwebtoken` in the
desktop crate; Argon2id + JWT in the backend auth module.

---

## Auth

### Key derivation

The existing vault flow already runs Argon2id to produce a `master_key`, which
is used to wrap a random `vault_key`. `auth_secret` is derived from that same
`master_key` using HKDF-Expand — no second Argon2id invocation needed:

```
master-password + salt  →  Argon2id  →  master_key   (existing, unchanged)
master_key              →  HKDF-Expand("espass:auth:v1", 32)  →  auth_secret
```

The two outputs are cryptographically independent because they use different
HKDF info strings. `vault_key` is a random key wrapped by `master_key`
(existing behaviour); `auth_secret` is deterministically derived and is
therefore always re-derivable from an unlocked vault without storing it.

### Account lifecycle

**Registration** (`POST /v1/auth/register`):
- Client sends `{ email, auth_secret_hash }` where `auth_secret_hash` is
  Argon2id(auth_secret, server-generated salt, default params).
- Server stores email + hash. Returns `{ vault_id, user_id }`.

**Login** (`POST /v1/auth/login`):
- Client sends `{ email, auth_secret }`.
- Server verifies against stored hash. Returns `{ jwt, refresh_token, vault_id }`.
- JWT lifetime: 15 minutes. Refresh token lifetime: 7 days (in-memory only).

### Token lifecycle

- JWT and refresh token live in `SyncState` in RAM only — never written to disk.
- Token is wiped when `lock_vault` is called.
- Background task checks expiry 60 seconds before JWT expires and calls
  `POST /v1/auth/refresh` with the refresh token to get a new JWT.
- If refresh fails (server unreachable, token revoked), sync pauses silently
  and `sync_status` reports `Unauthenticated`. Next vault unlock re-derives
  `auth_secret` and logs in again.

---

## Per-item Sync

### Encryption per credential

Each credential is encrypted individually before leaving the device:

```
plaintext  = serde_json::to_vec(credential)
aad        = format!("espass:item:v1:{vault_id}:{item_id}")
nonce      = random 12 bytes (OS CSPRNG)
ciphertext = AES-256-GCM(vault_key, nonce, plaintext, aad)

VaultItem {
    item_id:           credential.id (UUID),
    vault_id:          from SyncState,
    encrypted_payload: EncryptedPayload { nonce, ciphertext, aad_context: aad },
    revision:          0 (server assigns),
    base_revision:     last known server revision for this item,
    created_at:        credential.created_at,
    updated_at:        credential.updated_at,
    deleted_at:        None (or Some(now) for deletes),
    attachments:       vec![],
}
```

### Push (local → server)

Triggered by: manual Sync button, or 30 seconds after any vault mutation
(debounced — rapid edits produce one push, not many).

1. Load `VaultContents` from local vault.
2. Load `sync_state.json` to get known server revisions per item.
3. For each credential where `updated_at > last_pushed_at`:
   - Encrypt → `PUT /v1/vaults/{vault_id}/items/{item_id}` with JWT header.
   - On 204: update `last_pushed_at` in sync_state.
   - On 409 (conflict): server has a newer revision — skip push, flag item for
     pull on next sync cycle.
4. For each credential deleted since last sync: push with `deleted_at = now`.

### Pull (server → local)

Runs immediately after push in the same sync cycle.

1. `GET /v1/vaults/{vault_id}/items` — returns list of all items with
   `{ item_id, updated_at, revision, deleted_at }` (no ciphertext in list).
2. For each server item where `updated_at > local credential updated_at`
   (or item does not exist locally):
   - `GET /v1/vaults/{vault_id}/items/{item_id}` → decrypt → merge into
     `VaultContents`.
3. For each server item where `deleted_at` is set and local credential exists:
   - Remove from local `VaultContents`.
4. Save merged `VaultContents` via existing `save_contents`.
5. Update `sync_state.json` with new revision numbers and `last_synced_at`.

### Conflict resolution

Last-write-wins on `updated_at`. If two devices edit the same credential while
offline, the one with the later `updated_at` wins when both sync. No user
interaction required. The losing version is silently discarded.

### Sync state file

`sync_state.json` lives next to the vault file (not encrypted — contains no
secrets):

```json
{
  "server_url": "https://sync.example.com",
  "user_id": "uuid",
  "vault_id": "uuid",
  "last_synced_at": 1716912345,
  "items": {
    "item-uuid": { "server_revision": 4, "last_pushed_at": 1716912300 }
  }
}
```

---

## Backend Changes

### New endpoints

| Method | Path | Purpose |
|--------|------|---------|
| `POST` | `/v1/auth/register` | Create account, store hashed auth_secret |
| `POST` | `/v1/auth/login` | Verify auth_secret → issue JWT + refresh token |
| `POST` | `/v1/auth/refresh` | Exchange refresh token → new JWT |
| `GET`  | `/v1/vaults/{vault_id}/items` | List item metadata (no ciphertext) |

Existing `PUT/GET /v1/vaults/{vault_id}/items/{item_id}` endpoints gain JWT
middleware — requests without a valid JWT return 401.

### New file

`apps/backend/src/auth.rs` — register/login/refresh handlers, Argon2id
verification, JWT signing/verification with `jsonwebtoken` crate.

### JWT claims

```json
{ "sub": "user_id", "vault_id": "uuid", "exp": 1716912345, "iat": 1716911445 }
```

Signed with HMAC-SHA256 using a secret loaded from `ESPASS_JWT_SECRET` env var.

---

## Desktop Changes

### New file: `sync.rs`

Owns all sync logic. Public API consumed by `commands.rs`:

```rust
pub async fn sync_now(state: &AppState) -> Result<SyncResult, SyncError>
pub async fn configure(url: &str, email: &str, auth_secret: &[u8], state: &AppState) -> Result<(), SyncError>
pub fn get_status(state: &AppState) -> SyncStatus
```

`SyncResult` contains counts:

```rust
pub struct SyncResult {
    pub pushed: u32,
    pub pulled: u32,
    pub deleted: u32,
    pub conflicts_skipped: u32,
}
```

### State additions (`state.rs`)

```rust
pub struct SyncState {
    pub server_url: String,
    pub user_id: Uuid,
    pub vault_id: Uuid,
    pub jwt: String,              // in RAM only
    pub refresh_token: String,    // in RAM only
    pub jwt_expires_at: i64,
    pub last_synced_at: Option<i64>,
    pub status: SyncStatus,
}

pub enum SyncStatus {
    NotConfigured,
    Idle { last_synced_at: i64 },
    Syncing,
    Error(String),
    Unauthenticated,
}
```

`AppState` gains `pub sync: Mutex<Option<SyncState>>`.

### New Tauri commands

```rust
#[tauri::command]
pub async fn sync_configure(server_url: String, email: String, password: String, register: bool, state: State<AppState>) -> Result<(), String>

#[tauri::command]
pub async fn sync_now(state: State<AppState>) -> Result<SyncResult, String>

#[tauri::command]
pub fn get_sync_status(state: State<AppState>) -> SyncStatus
```

### Cargo.toml additions

```toml
reqwest  = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
jsonwebtoken = "9"
```

---

## UI

### Topbar

```
[ESPASS]  [search………]  [Sync ↻]  [Tools ▾]  [Lock]  [+ Add]
```

- **Sync ↻** button: clicking triggers `sync_now`. While syncing, shows spinner.
- Below button: small status line — *"Synced 14:32"* or *"Sync failed"* or
  *"Not configured"*.

### Sync settings modal

Accessible via **Tools ▾ → Sync-indstillinger**.

```
Server URL   [https://…………………………………]
E-mail       [user@example.com…………………]
Password     [••••••••••••••••••••••••]
             [ Opret konto ]  [ Log ind ]
```

On success: modal closes, status updates to *"Synced just now"*, automatic
sync starts. On error: inline error message.

---

## Security Notes

- `auth_secret` is never stored — derived fresh from master password at each unlock.
- JWT and refresh token live in `AppState.sync` (RAM), wiped on `lock_vault`.
- `sync_state.json` contains no secrets — only UUIDs, timestamps, and server URL.
- Each credential has its own nonce; AAD binds ciphertext to vault_id + item_id,
  preventing cross-vault or cross-item ciphertext substitution.
- Backend never receives master password, vault key, or plaintext credentials.
- TLS required for all sync traffic (`rustls-tls` feature, no native TLS).
- Soft-deletes ensure deleted credentials propagate to all devices.

---

## Files Changed

| File | Change |
|------|--------|
| `apps/backend/src/auth.rs` | New — register, login, refresh handlers |
| `apps/backend/src/main.rs` | Add auth routes, JWT middleware on item endpoints |
| `apps/desktop/src-tauri/src/sync.rs` | New — HTTP client, push/pull, token refresh |
| `apps/desktop/src-tauri/src/state.rs` | Add `SyncState`, `SyncStatus`, `sync` field on `AppState` |
| `apps/desktop/src-tauri/src/commands.rs` | Add `sync_configure`, `sync_now`, `get_sync_status` |
| `apps/desktop/src-tauri/src/lib.rs` | Register 3 new commands |
| `apps/desktop/src-tauri/Cargo.toml` | Add `reqwest`, `jsonwebtoken` |
| `apps/desktop/dist/app.js` | Sync button, settings modal, status line |
| `apps/desktop/dist/style.css` | Settings modal styles, sync status styles |
