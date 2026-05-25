# ESPASS Desktop Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add edit credential, configurable password generator, and CSV/JSON import/export to the ESPASS Tauri desktop app.

**Architecture:** All new logic follows the existing pattern — Rust Tauri commands handle data/crypto, `dist/app.js` is vanilla JS calling `invoke()`. The `csv` crate is added for robust CSV parsing. No new Tauri plugins are needed.

**Tech Stack:** Rust 1.95, Tauri 2, `csv = "1.3"`, `espass-crypto-core` (random_vec), vanilla JS + HTML

---

## File Map

| File | Change |
|---|---|
| `apps/desktop/src-tauri/Cargo.toml` | Add `csv = "1.3"` |
| `apps/desktop/src-tauri/src/commands.rs` | Add `update_credential`, `generate_password`, `import_credentials`, `export_credentials_csv`, `export_credentials_json`, `ImportSummary`, helper `detect_csv_columns`, unit tests |
| `apps/desktop/src-tauri/src/lib.rs` | Register 5 new commands |
| `apps/desktop/dist/app.js` | Edit modal mode, generator panel, tools dropdown, import/export UI, toast |
| `apps/desktop/dist/style.css` | Generator panel, tools dropdown, toast styles |

---

## Task 1: update_credential + generate_password Rust commands

**Files:**
- Modify: `apps/desktop/src-tauri/src/commands.rs` (append after `delete_credential`)

- [ ] **Step 1: Add `update_credential` command**

Append to the end of `apps/desktop/src-tauri/src/commands.rs` (before the closing of the file, after `delete_credential`):

```rust
/// Updates an existing credential's fields.
#[tauri::command]
pub fn update_credential(
    id: String,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    state: State<AppState>,
) -> Result<(), String> {
    let (key_bytes, vault_id) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        let vid = secrets.vault_id().ok_or("Vault is locked")?;
        (kb, vid)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let mut contents = load_contents(&vault_key, &state)?;
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let cred = contents
        .credentials
        .iter_mut()
        .find(|c| c.id == id)
        .ok_or_else(|| "Credential not found".to_string())?;
    cred.title = title;
    cred.username = username;
    cred.password = password;
    cred.url = url;
    cred.updated_at = now;

    let mut meta = load_meta(&state)?;
    save_contents(&vault_key, vault_id, &contents, &mut meta, &state)?;
    Ok(())
}
```

- [ ] **Step 2: Add `generate_password` command**

Append immediately after `update_credential`:

```rust
/// Generates a cryptographically secure password from the selected character sets.
///
/// Uses rejection sampling on OS-CSPRNG bytes to eliminate modulo bias.
/// `length` is clamped to [8, 64].
#[tauri::command]
pub fn generate_password(
    length: u8,
    upper: bool,
    lower: bool,
    digits: bool,
    symbols: bool,
) -> Result<String, String> {
    use espass_crypto_core::random_vec;

    let mut charset = String::new();
    if upper   { charset.push_str("ABCDEFGHIJKLMNOPQRSTUVWXYZ"); }
    if lower   { charset.push_str("abcdefghijklmnopqrstuvwxyz"); }
    if digits  { charset.push_str("0123456789"); }
    if symbols { charset.push_str("!@#$%^&*()-_=+[]{}|;:,.<>?"); }

    if charset.is_empty() {
        return Err("no character set selected".to_string());
    }

    let length = length.clamp(8, 64) as usize;
    let chars: Vec<char> = charset.chars().collect();
    let n = chars.len();
    // Rejection-sampling threshold: discard bytes >= this value to eliminate bias.
    let threshold = 256 - (256 % n);

    let mut result = String::with_capacity(length);
    while result.len() < length {
        let batch = random_vec(length * 2).map_err(|_| "random generation failed".to_string())?;
        for byte in batch {
            if result.len() >= length { break; }
            if (byte as usize) < threshold {
                result.push(chars[byte as usize % n]);
            }
        }
    }
    Ok(result)
}
```

- [ ] **Step 3: Add unit tests**

Append a `#[cfg(test)]` block at the very end of `commands.rs`:

```rust
#[cfg(test)]
mod commands_tests {
    use super::*;

    #[test]
    fn generate_password_correct_length() {
        let pw = generate_password(20, true, true, true, true).unwrap();
        assert_eq!(pw.len(), 20);
    }

    #[test]
    fn generate_password_only_upper() {
        let pw = generate_password(16, true, false, false, false).unwrap();
        assert!(pw.chars().all(|c| c.is_ascii_uppercase()), "unexpected char in: {pw}");
    }

    #[test]
    fn generate_password_clamps_length() {
        let pw = generate_password(4, true, true, false, false).unwrap();
        assert_eq!(pw.len(), 8, "length below 8 should be clamped to 8");

        let pw2 = generate_password(200, true, true, false, false).unwrap();
        assert_eq!(pw2.len(), 64, "length above 64 should be clamped to 64");
    }

    #[test]
    fn generate_password_empty_charset_errors() {
        let err = generate_password(16, false, false, false, false).unwrap_err();
        assert_eq!(err, "no character set selected");
    }
}
```

- [ ] **Step 4: Run tests**

```powershell
$env:PATH += ";$env:USERPROFILE\.cargo\bin"
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml 2>&1
```

Expected: `test commands_tests::generate_password_correct_length ... ok` (and 3 more)

- [ ] **Step 5: Commit**

```powershell
git add apps/desktop/src-tauri/src/commands.rs
git commit -m "feat(desktop): add update_credential and generate_password commands"
```

---

## Task 2: Import/export Rust commands

**Files:**
- Modify: `apps/desktop/src-tauri/Cargo.toml`
- Modify: `apps/desktop/src-tauri/src/commands.rs`

- [ ] **Step 1: Add csv dependency**

In `apps/desktop/src-tauri/Cargo.toml`, add after `serde_json`:

```toml
csv = "1.3"
```

- [ ] **Step 2: Add `ImportSummary` struct and `detect_csv_columns` helper**

Append to `commands.rs` (after the `commands_tests` block but OUTSIDE it):

```rust
// ---------------------------------------------------------------------------
// Import / Export
// ---------------------------------------------------------------------------

/// Summary returned after a CSV import.
#[derive(Debug, serde::Serialize)]
pub struct ImportSummary {
    pub imported: u32,
    pub skipped: u32,
    pub errors: u32,
}

/// Returns (name_col, url_col, user_col, pass_col) indices for a CSV header row.
/// `url_col` is `None` when no URL column exists.
fn detect_csv_columns(
    headers: &csv::StringRecord,
) -> Result<(usize, Option<usize>, usize, usize), String> {
    let find = |candidates: &[&str]| -> Option<usize> {
        candidates.iter().find_map(|c| {
            headers.iter().position(|h| h.eq_ignore_ascii_case(c))
        })
    };

    let name = find(&["name", "title"]).ok_or("Cannot detect name column")?;
    let url  = find(&["url", "login_uri", "formActionOrigin"]);
    let user = find(&["username", "login_username"]).ok_or("Cannot detect username column")?;
    let pass = find(&["password", "login_password"]).ok_or("Cannot detect password column")?;

    Ok((name, url, user, pass))
}
```

- [ ] **Step 3: Add `import_credentials` command**

Append after `detect_csv_columns`:

```rust
/// Parses a CSV string (Chrome / Firefox / Bitwarden format) and adds
/// credentials to the unlocked vault. Returns an import summary.
#[tauri::command]
pub fn import_credentials(
    csv_text: String,
    state: State<AppState>,
) -> Result<ImportSummary, String> {
    let (key_bytes, vault_id) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        let vid = secrets.vault_id().ok_or("Vault is locked")?;
        (kb, vid)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let mut contents = load_contents(&vault_key, &state)?;
    let mut summary = ImportSummary { imported: 0, skipped: 0, errors: 0 };
    let now = time::OffsetDateTime::now_utc().unix_timestamp();

    let mut rdr = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_text.as_bytes());

    let headers = rdr.headers().map_err(|e| e.to_string())?.clone();
    let (col_name, col_url, col_user, col_pass) = detect_csv_columns(&headers)?;

    for result in rdr.records() {
        let record = match result {
            Ok(r) => r,
            Err(_) => { summary.errors += 1; continue; }
        };

        let title    = record.get(col_name).unwrap_or("").trim().to_string();
        let username = record.get(col_user).unwrap_or("").trim().to_string();
        let password = record.get(col_pass).unwrap_or("").trim().to_string();
        let url = col_url
            .and_then(|i| record.get(i))
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        if username.is_empty() && password.is_empty() {
            summary.skipped += 1;
            continue;
        }

        // Skip duplicates (same title + username, case-insensitive).
        let is_dup = contents.credentials.iter().any(|c| {
            c.title.eq_ignore_ascii_case(&title) && c.username.eq_ignore_ascii_case(&username)
        });
        if is_dup {
            summary.skipped += 1;
            continue;
        }

        contents.credentials.push(Credential {
            id: Uuid::new_v4().to_string(),
            title: if title.is_empty() { username.clone() } else { title },
            username,
            password,
            url,
            created_at: now,
            updated_at: now,
        });
        summary.imported += 1;
    }

    if summary.imported > 0 {
        let mut meta = load_meta(&state)?;
        save_contents(&vault_key, vault_id, &contents, &mut meta, &state)?;
    }

    Ok(summary)
}
```

- [ ] **Step 4: Add `export_credentials_csv` command**

Append after `import_credentials`:

```rust
/// Exports all credentials as a Chrome-compatible CSV string (plaintext).
#[tauri::command]
pub fn export_credentials_csv(state: State<AppState>) -> Result<String, String> {
    let (key_bytes,) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        (kb,)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);
    let contents = load_contents(&vault_key, &state)?;

    let mut wtr = csv::Writer::from_writer(vec![]);
    wtr.write_record(["name", "url", "username", "password"])
        .map_err(|e| e.to_string())?;
    for c in &contents.credentials {
        wtr.write_record([
            c.title.as_str(),
            c.url.as_deref().unwrap_or(""),
            c.username.as_str(),
            c.password.as_str(),
        ])
        .map_err(|e| e.to_string())?;
    }
    let data = wtr.into_inner().map_err(|e| e.to_string())?;
    String::from_utf8(data).map_err(|e| e.to_string())
}
```

- [ ] **Step 5: Add `export_credentials_json` command**

Append after `export_credentials_csv`:

```rust
/// Exports all credentials as a pretty-printed JSON string (plaintext backup).
#[tauri::command]
pub fn export_credentials_json(state: State<AppState>) -> Result<String, String> {
    let (key_bytes,) = {
        let secrets = state.secrets.lock().map_err(|e| e.to_string())?;
        let key = secrets.vault_key().map_err(|_| "Vault is locked".to_string())?;
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        (kb,)
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);
    let contents = load_contents(&vault_key, &state)?;
    serde_json::to_string_pretty(&contents).map_err(|e| e.to_string())
}
```

- [ ] **Step 6: Add CSV column detection unit tests**

Append inside a new `#[cfg(test)]` block at the end of `commands.rs`:

```rust
#[cfg(test)]
mod import_tests {
    use super::*;

    fn make_headers(cols: &[&str]) -> csv::StringRecord {
        csv::StringRecord::from(cols.to_vec())
    }

    #[test]
    fn detect_chrome_format() {
        let h = make_headers(&["name", "url", "username", "password"]);
        let (name, url, user, pass) = detect_csv_columns(&h).unwrap();
        assert_eq!((name, url, user, pass), (0, Some(1), 2, 3));
    }

    #[test]
    fn detect_bitwarden_format() {
        let h = make_headers(&["folder","favorite","type","name","notes","fields",
                               "reprompt","login_uri","login_username","login_password","login_totp"]);
        let (name, url, user, pass) = detect_csv_columns(&h).unwrap();
        assert_eq!(name, 3);
        assert_eq!(url, Some(7));
        assert_eq!(user, 8);
        assert_eq!(pass, 9);
    }

    #[test]
    fn detect_missing_password_errors() {
        let h = make_headers(&["name", "url", "username"]);
        assert!(detect_csv_columns(&h).is_err());
    }

    #[test]
    fn detect_no_url_column() {
        let h = make_headers(&["name", "username", "password"]);
        let (_, url, _, _) = detect_csv_columns(&h).unwrap();
        assert_eq!(url, None);
    }
}
```

- [ ] **Step 7: Run all tests**

```powershell
$env:PATH += ";$env:USERPROFILE\.cargo\bin"
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml 2>&1
```

Expected: 8 tests pass (4 from Task 1 + 4 new)

- [ ] **Step 8: Commit**

```powershell
git add apps/desktop/src-tauri/Cargo.toml apps/desktop/src-tauri/src/commands.rs
git commit -m "feat(desktop): add import_credentials, export_credentials_csv/json commands"
```

---

## Task 3: Register new commands in lib.rs

**Files:**
- Modify: `apps/desktop/src-tauri/src/lib.rs`

- [ ] **Step 1: Register all 5 new commands**

Replace the entire contents of `apps/desktop/src-tauri/src/lib.rs` with:

```rust
//! ESPASS desktop application library entry point.

mod commands;
mod state;

pub use state::AppState;

/// Entry point for the Tauri application.
pub fn run() {
    tauri::Builder::default()
        .manage(state::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::vault_exists,
            commands::create_vault,
            commands::unlock_vault,
            commands::lock_vault,
            commands::get_session_status,
            commands::list_credentials,
            commands::add_credential,
            commands::get_credential,
            commands::delete_credential,
            commands::update_credential,
            commands::generate_password,
            commands::import_credentials,
            commands::export_credentials_csv,
            commands::export_credentials_json,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
```

- [ ] **Step 2: Verify cargo check passes**

```powershell
$env:PATH += ";$env:USERPROFILE\.cargo\bin"
cargo check --manifest-path apps/desktop/src-tauri/Cargo.toml 2>&1
```

Expected: `Finished dev` with only doc warnings, no errors.

- [ ] **Step 3: Commit**

```powershell
git add apps/desktop/src-tauri/src/lib.rs
git commit -m "feat(desktop): register update_credential, generate_password, import/export commands"
```

---

## Task 4: Frontend — edit credential modal

**Files:**
- Modify: `apps/desktop/dist/app.js`

The existing `state` object and modal machinery need to support an edit mode.
Make the following changes to `app.js` **in order**. Each step describes the
exact change to make.

- [ ] **Step 1: Extend state with edit fields**

Find this line in `app.js`:

```javascript
const state = {
  credentials: [],
  selectedId: null,
  search: '',
  showAddModal: false,
  revealPassword: false,
};
```

Replace with:

```javascript
const state = {
  credentials: [],
  selectedId: null,
  search: '',
  showAddModal: false,
  modalMode: 'add',   // 'add' | 'edit'
  editingId: null,    // credential id being edited
  revealPassword: false,
};
```

- [ ] **Step 2: Add Edit button to the detail panel HTML**

Find `renderDetailHTML`. Locate this line inside it:

```javascript
      <div class="detail__footer">
        <button class="btn btn--danger" id="delete-btn" data-id="${esc(cred.id)}">Delete</button>
      </div>
```

Replace with:

```javascript
      <div class="detail__footer">
        <button class="btn btn--ghost" id="edit-btn" data-id="${esc(cred.id)}">Edit</button>
        <button class="btn btn--danger" id="delete-btn" data-id="${esc(cred.id)}">Delete</button>
      </div>
```

- [ ] **Step 3: Make the modal title and submit label dynamic**

Find `renderAddModalHTML`. Replace its opening lines:

```javascript
function renderAddModalHTML() {
  return `
    <div class="modal-overlay" id="modal-overlay">
      <div class="modal" role="dialog" aria-modal="true" aria-label="Add credential">
        <h3 class="modal__title">Add Credential</h3>
        <form id="add-form" autocomplete="off" novalidate>
          <div class="field">
            <label for="add-title">Title <span class="required">*</span></label>
            <input id="add-title" type="text" placeholder="e.g. GitHub" required autofocus>
          </div>
          <div class="field">
            <label for="add-username">Username</label>
            <input id="add-username" type="text" placeholder="e.g. user@example.com" autocomplete="off">
          </div>
```

With:

```javascript
function renderAddModalHTML() {
  const isEdit = state.modalMode === 'edit';
  const editing = isEdit ? state.credentials.find(c => c.id === state.editingId) : null;
  const title = editing?.title ?? '';
  const username = editing?.username ?? '';
  const url = editing?.url ?? '';

  return `
    <div class="modal-overlay" id="modal-overlay">
      <div class="modal" role="dialog" aria-modal="true" aria-label="${isEdit ? 'Edit credential' : 'Add credential'}">
        <h3 class="modal__title">${isEdit ? 'Edit Credential' : 'Add Credential'}</h3>
        <form id="add-form" autocomplete="off" novalidate>
          <div class="field">
            <label for="add-title">Title <span class="required">*</span></label>
            <input id="add-title" type="text" placeholder="e.g. GitHub" required autofocus value="${esc(title)}">
          </div>
          <div class="field">
            <label for="add-username">Username</label>
            <input id="add-username" type="text" placeholder="e.g. user@example.com" autocomplete="off" value="${esc(username)}">
          </div>
```

- [ ] **Step 4: Pre-fill URL field in modal**

In the same `renderAddModalHTML`, find:

```javascript
            <input id="add-url" type="url" placeholder="https://example.com" autocomplete="off">
```

Replace with:

```javascript
            <input id="add-url" type="url" placeholder="https://example.com" autocomplete="off" value="${esc(url)}">
```

- [ ] **Step 5: Make submit button label dynamic**

In `renderAddModalHTML`, find:

```javascript
            <button type="submit" class="btn btn--primary" id="add-save">Save</button>
```

Replace with:

```javascript
            <button type="submit" class="btn btn--primary" id="add-save">${isEdit ? 'Save changes' : 'Save'}</button>
```

- [ ] **Step 6: Handle edit vs add on form submit**

In `bindVaultEvents`, find the add form submit handler. Locate this block:

```javascript
      try {
        const id = await invoke('add_credential', { title, username, password, url });
        state.credentials.push({ id, title, username, url });
        state.selectedId = id;
        state.revealPassword = false;
        closeAddModal();
      } catch (err) {
        showError(errEl, `Error: ${err}`);
        saveBtn.disabled = false;
        saveBtn.textContent = 'Save';
      }
```

Replace with:

```javascript
      try {
        if (state.modalMode === 'edit' && state.editingId) {
          await invoke('update_credential', { id: state.editingId, title, username, password, url });
          const idx = state.credentials.findIndex(c => c.id === state.editingId);
          if (idx !== -1) state.credentials[idx] = { ...state.credentials[idx], title, username, url };
          state.selectedId = state.editingId;
        } else {
          const id = await invoke('add_credential', { title, username, password, url });
          state.credentials.push({ id, title, username, url });
          state.selectedId = id;
        }
        state.revealPassword = false;
        closeAddModal();
      } catch (err) {
        showError(errEl, `Error: ${err}`);
        saveBtn.disabled = false;
        saveBtn.textContent = state.modalMode === 'edit' ? 'Save changes' : 'Save';
      }
```

- [ ] **Step 7: Wire the Edit button in bindVaultEvents**

In `bindVaultEvents`, find the delete button binding:

```javascript
  // Detail panel: delete
  $('#delete-btn')?.addEventListener('click', async (e) => {
```

Add the following BEFORE that block:

```javascript
  // Detail panel: edit
  $('#edit-btn')?.addEventListener('click', async (e) => {
    const id = e.currentTarget.dataset.id;
    // Fetch full credential (including password) for pre-fill
    try {
      const cred = await invoke('get_credential', { id });
      // Temporarily store password on in-memory cred for pre-fill
      const found = state.credentials.find(c => c.id === id);
      if (found) found._password = cred.password;
      state.modalMode = 'edit';
      state.editingId = id;
      state.showAddModal = true;
      renderUnlocked();
      $('#add-password').value = cred.password ?? '';
      $('#add-title')?.focus();
    } catch (err) {
      alert(`Could not load credential: ${err}`);
    }
  });

```

- [ ] **Step 8: Reset modal state on close**

Find `closeAddModal`:

```javascript
function closeAddModal() {
  document.removeEventListener('keydown', handleModalEscape);
  state.showAddModal = false;
  renderUnlocked();
}
```

Replace with:

```javascript
function closeAddModal() {
  document.removeEventListener('keydown', handleModalEscape);
  state.showAddModal = false;
  state.modalMode = 'add';
  state.editingId = null;
  renderUnlocked();
}
```

- [ ] **Step 9: Wire Add button to reset modal mode**

Find:

```javascript
  // Add button
  $('#add-btn')?.addEventListener('click', () => {
    state.showAddModal = true;
    renderUnlocked();
    $('#add-title')?.focus();
  });
```

Replace with:

```javascript
  // Add button
  $('#add-btn')?.addEventListener('click', () => {
    state.modalMode = 'add';
    state.editingId = null;
    state.showAddModal = true;
    renderUnlocked();
    $('#add-title')?.focus();
  });
```

- [ ] **Step 10: Manual test**

Run `npx @tauri-apps/cli dev` from `apps/desktop/`. Add a credential, click it, click Edit. Verify the modal opens pre-filled. Change a field, click Save changes. Verify the list updates.

- [ ] **Step 11: Commit**

```powershell
git add apps/desktop/dist/app.js
git commit -m "feat(desktop): edit credential modal with pre-fill"
```

---

## Task 5: Frontend — password generator panel

**Files:**
- Modify: `apps/desktop/dist/app.js`
- Modify: `apps/desktop/dist/style.css`

- [ ] **Step 1: Add generator styles to style.css**

Append to the end of `apps/desktop/dist/style.css`:

```css
/* ── Password generator panel ───────────────────────────────── */
.gen-panel {
  background: var(--surface); border: 1px solid var(--border);
  border-radius: var(--radius); padding: 12px; margin-top: 6px;
  display: flex; flex-direction: column; gap: 10px;
}
.gen-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.gen-length { display: flex; align-items: center; gap: 8px; flex: 1; min-width: 160px; }
.gen-length input[type="range"] { flex: 1; accent-color: var(--accent); }
.gen-length-val { font-size: 13px; font-weight: 600; color: var(--accent); min-width: 24px; text-align: right; }
.gen-checks { display: flex; gap: 10px; flex-wrap: wrap; }
.gen-checks label { display: flex; align-items: center; gap: 4px; font-size: 12px; cursor: pointer; }
.gen-checks input[type="checkbox"] { accent-color: var(--accent); }
```

- [ ] **Step 2: Add generator state to the modal**

The generator state is local to the modal (not in `state`). We use a plain JS object scoped to the `bindVaultEvents` call. At the top of `bindVaultEvents`, after the opening `{`, add:

```javascript
  // Generator local state (reset each time modal opens)
  const gen = { length: 20, upper: true, lower: true, digits: true, symbols: true };
```

- [ ] **Step 3: Add generator toggle button to modal HTML**

In `renderAddModalHTML`, find the password field section:

```javascript
          <div class="field">
            <label for="add-password">Password</label>
            <div class="input-row">
              <input id="add-password" type="password" placeholder="Password" autocomplete="new-password">
              <button type="button" class="btn btn--sm" id="add-pw-toggle">Show</button>
            </div>
          </div>
```

Replace with:

```javascript
          <div class="field">
            <label for="add-password">Password</label>
            <div class="input-row">
              <input id="add-password" type="password" placeholder="Password" autocomplete="new-password">
              <button type="button" class="btn btn--sm" id="add-pw-toggle">Show</button>
              <button type="button" class="btn btn--sm" id="gen-toggle">Generate</button>
            </div>
            <div class="gen-panel" id="gen-panel" style="display:none">
              <div class="gen-length">
                <input type="range" id="gen-len" min="8" max="64" value="20">
                <span class="gen-length-val" id="gen-len-val">20</span>
              </div>
              <div class="gen-checks">
                <label><input type="checkbox" id="gen-upper" checked> A–Z</label>
                <label><input type="checkbox" id="gen-lower" checked> a–z</label>
                <label><input type="checkbox" id="gen-digits" checked> 0–9</label>
                <label><input type="checkbox" id="gen-symbols" checked> !@#</label>
              </div>
              <button type="button" class="btn btn--primary btn--sm" id="gen-go">Generate password</button>
            </div>
          </div>
```

- [ ] **Step 4: Wire generator events in bindVaultEvents**

Find the password toggle wiring inside the `if (state.showAddModal)` block:

```javascript
    // Password toggle
    $('#add-pw-toggle')?.addEventListener('click', () => {
      const pwInput = $('#add-password');
      const btn = $('#add-pw-toggle');
      if (pwInput.type === 'password') {
        pwInput.type = 'text';
        btn.textContent = 'Hide';
      } else {
        pwInput.type = 'password';
        btn.textContent = 'Show';
      }
    });
```

Add the following immediately AFTER that block:

```javascript
    // Generator toggle
    $('#gen-toggle')?.addEventListener('click', () => {
      const panel = $('#gen-panel');
      if (!panel) return;
      panel.style.display = panel.style.display === 'none' ? '' : 'none';
    });

    // Length slider
    $('#gen-len')?.addEventListener('input', (e) => {
      gen.length = parseInt(e.target.value, 10);
      const val = $('#gen-len-val');
      if (val) val.textContent = gen.length;
    });

    // Checkboxes — prevent unchecking the last one
    ['gen-upper', 'gen-lower', 'gen-digits', 'gen-symbols'].forEach(id => {
      $(`#${id}`)?.addEventListener('change', () => {
        gen.upper   = !!$('#gen-upper')?.checked;
        gen.lower   = !!$('#gen-lower')?.checked;
        gen.digits  = !!$('#gen-digits')?.checked;
        gen.symbols = !!$('#gen-symbols')?.checked;
        const anyChecked = gen.upper || gen.lower || gen.digits || gen.symbols;
        if (!anyChecked) {
          // Re-check the box that was just unchecked
          const box = $(`#${id}`);
          if (box) box.checked = true;
          gen[id.replace('gen-', '')] = true;
        }
      });
    });

    // Generate button
    $('#gen-go')?.addEventListener('click', async () => {
      const btn = $('#gen-go');
      if (btn) { btn.disabled = true; btn.textContent = 'Generating…'; }
      try {
        const pw = await invoke('generate_password', {
          length: gen.length,
          upper: gen.upper,
          lower: gen.lower,
          digits: gen.digits,
          symbols: gen.symbols,
        });
        const pwInput = $('#add-password');
        if (pwInput) {
          pwInput.value = pw;
          pwInput.type = 'text'; // reveal so user can see it
          const toggle = $('#add-pw-toggle');
          if (toggle) toggle.textContent = 'Hide';
        }
      } catch (err) {
        alert(`Generator error: ${err}`);
      } finally {
        if (btn) { btn.disabled = false; btn.textContent = 'Generate password'; }
      }
    });
```

- [ ] **Step 5: Manual test**

In the app, click "+ Add". Click "Generate". Verify the panel appears. Slide the length, uncheck a category, click "Generate password". Verify the password field fills with a password of the correct length. Try unchecking all boxes — verify the last one stays checked.

- [ ] **Step 6: Commit**

```powershell
git add apps/desktop/dist/app.js apps/desktop/dist/style.css
git commit -m "feat(desktop): configurable password generator panel in modal"
```

---

## Task 6: Frontend — tools menu, import/export, toast

**Files:**
- Modify: `apps/desktop/dist/app.js`
- Modify: `apps/desktop/dist/style.css`

- [ ] **Step 1: Add tools dropdown + toast styles**

Append to end of `apps/desktop/dist/style.css`:

```css
/* ── Tools dropdown ─────────────────────────────────────────── */
.tools-wrap { position: relative; }
.tools-menu {
  position: absolute; top: calc(100% + 6px); right: 0;
  background: var(--surface); border: 1px solid var(--border);
  border-radius: var(--radius); min-width: 160px; z-index: 50;
  box-shadow: 0 8px 24px rgba(0,0,0,.4);
  display: flex; flex-direction: column; overflow: hidden;
}
.tools-menu button {
  background: none; border: none; padding: 10px 14px;
  text-align: left; color: var(--text); font-size: 13px; cursor: pointer;
}
.tools-menu button:hover { background: var(--surface2); }
.tools-menu hr { border: none; border-top: 1px solid var(--border); margin: 0; }

/* ── Toast ──────────────────────────────────────────────────── */
@keyframes toast-in  { from { opacity: 0; transform: translateY(12px); } to { opacity: 1; transform: none; } }
@keyframes toast-out { from { opacity: 1; } to { opacity: 0; } }
.toast {
  position: fixed; bottom: 24px; left: 50%; transform: translateX(-50%);
  background: var(--surface2); border: 1px solid var(--border);
  border-radius: var(--radius); padding: 10px 18px;
  font-size: 13px; color: var(--text); z-index: 200;
  animation: toast-in .2s ease;
  box-shadow: 0 4px 16px rgba(0,0,0,.4);
}
.toast--success { border-color: #3a6a3a; color: #6fba6f; }
.toast--error   { border-color: rgba(224,82,82,.4); color: #f08; }
```

- [ ] **Step 2: Add showToast helper to app.js**

Find the `showError` function near the bottom of `app.js`:

```javascript
function showError(el, msg) {
  el.textContent = msg;
  el.hidden = false;
}
```

Add immediately after it:

```javascript
function showToast(msg, type = 'success', durationMs = 3000) {
  const existing = document.querySelector('.toast');
  if (existing) existing.remove();
  const el = document.createElement('div');
  el.className = `toast toast--${type}`;
  el.textContent = msg;
  document.body.appendChild(el);
  setTimeout(() => {
    el.style.animation = 'toast-out .3s ease forwards';
    setTimeout(() => el.remove(), 300);
  }, durationMs);
}
```

- [ ] **Step 3: Add hidden file input to the vault screen**

In `renderUnlocked`, find the closing `</div>` of the vault screen (the last line of the app.innerHTML template, just before the closing backtick). Insert the hidden file input before the closing `</div>`:

Find:

```javascript
      <!-- Add modal -->
      ${state.showAddModal ? renderAddModalHTML() : ''}
    </div>`;
```

Replace with:

```javascript
      <!-- Add modal -->
      ${state.showAddModal ? renderAddModalHTML() : ''}
      <!-- Hidden import file input -->
      <input type="file" id="import-file-input" accept=".csv" style="display:none">
    </div>`;
```

- [ ] **Step 4: Add Tools button to topbar HTML**

In `renderUnlocked`, find the topbar actions div:

```javascript
        <div class="topbar__actions">
          <button class="btn btn--ghost" id="lock-btn">Lock</button>
          <button class="btn btn--primary" id="add-btn">+ Add</button>
        </div>
```

Replace with:

```javascript
        <div class="topbar__actions">
          <div class="tools-wrap">
            <button class="btn btn--ghost" id="tools-btn">Tools ▾</button>
            <div class="tools-menu" id="tools-menu" style="display:none">
              <button id="import-csv-btn">Import CSV</button>
              <hr>
              <button id="export-csv-btn">Export CSV</button>
              <button id="export-json-btn">Export JSON</button>
            </div>
          </div>
          <button class="btn btn--ghost" id="lock-btn">Lock</button>
          <button class="btn btn--primary" id="add-btn">+ Add</button>
        </div>
```

- [ ] **Step 5: Add downloadBlob helper to app.js**

Add this function near the top of `app.js` (after the `copyText` function):

```javascript
function downloadBlob(filename, text, mime = 'text/plain') {
  const blob = new Blob([text], { type: mime });
  const url  = URL.createObjectURL(blob);
  const a    = document.createElement('a');
  a.href = url; a.download = filename;
  document.body.appendChild(a);
  a.click();
  setTimeout(() => { URL.revokeObjectURL(url); a.remove(); }, 1000);
}
```

- [ ] **Step 6: Wire tools menu events in bindVaultEvents**

Find the lock button event binding:

```javascript
  // Lock
  $('#lock-btn')?.addEventListener('click', async () => {
```

Add the following BEFORE that binding:

```javascript
  // Tools dropdown toggle
  $('#tools-btn')?.addEventListener('click', (e) => {
    e.stopPropagation();
    const menu = $('#tools-menu');
    if (!menu) return;
    menu.style.display = menu.style.display === 'none' ? '' : 'none';
  });
  // Close tools menu when clicking outside
  document.addEventListener('click', () => {
    const menu = $('#tools-menu');
    if (menu) menu.style.display = 'none';
  }, { once: false, capture: true });

  // Import CSV
  $('#import-csv-btn')?.addEventListener('click', () => {
    $('#tools-menu').style.display = 'none';
    $('#import-file-input')?.click();
  });

  $('#import-file-input')?.addEventListener('change', async (e) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    e.target.value = ''; // reset so same file can be re-imported
    try {
      const summary = await invoke('import_credentials', { csvText: text });
      state.credentials = await invoke('list_credentials');
      renderUnlocked();
      showToast(`Imported ${summary.imported}, skipped ${summary.skipped} duplicates${summary.errors ? `, ${summary.errors} errors` : ''}`);
    } catch (err) {
      showToast(`Import failed: ${err}`, 'error');
    }
  });

  // Export CSV
  $('#export-csv-btn')?.addEventListener('click', async () => {
    $('#tools-menu').style.display = 'none';
    if (!confirm('This file will contain your passwords in plaintext.\n\nStore it in a secure location.')) return;
    try {
      const csv = await invoke('export_credentials_csv');
      downloadBlob('espass-export.csv', csv, 'text/csv');
    } catch (err) {
      showToast(`Export failed: ${err}`, 'error');
    }
  });

  // Export JSON
  $('#export-json-btn')?.addEventListener('click', async () => {
    $('#tools-menu').style.display = 'none';
    if (!confirm('This file will contain your passwords in plaintext.\n\nStore it in a secure location.')) return;
    try {
      const json = await invoke('export_credentials_json');
      downloadBlob('espass-export.json', json, 'application/json');
    } catch (err) {
      showToast(`Export failed: ${err}`, 'error');
    }
  });

```

- [ ] **Step 7: Manual test**

1. Run `npx @tauri-apps/cli dev` from `apps/desktop/`.
2. Unlock vault. Click **Tools ▾** — verify dropdown appears with Import/Export options.
3. Click **Export CSV** — confirm warning dialog, verify `espass-export.csv` downloads.
4. Open the CSV — verify it has `name,url,username,password` headers and your credentials.
5. Click **Export JSON** — verify `espass-export.json` downloads with `{"credentials":[...]}`.
6. Add 2 test credentials. Export CSV. Delete them. Import the CSV back — verify toast shows "Imported 2, skipped 0 duplicates".
7. Import again — verify toast shows "Imported 0, skipped 2 duplicates".

- [ ] **Step 8: Commit**

```powershell
git add apps/desktop/dist/app.js apps/desktop/dist/style.css
git commit -m "feat(desktop): tools menu, CSV/JSON import/export, toast notifications"
```

---

## Done

All features are implemented. Run `npx @tauri-apps/cli dev` and test the full flow:
- Edit an existing credential
- Generate a password with custom settings
- Export credentials to CSV and re-import them

Run `cargo test` one final time to confirm all 8 unit tests still pass.
