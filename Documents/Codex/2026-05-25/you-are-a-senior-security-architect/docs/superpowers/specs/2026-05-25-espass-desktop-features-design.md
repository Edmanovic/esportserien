# ESPASS Desktop: Edit, Generator & Import/Export Design

## Goal

Add three features to the existing ESPASS desktop app (Tauri + vanilla JS):
1. Edit existing credential
2. Configurable password generator
3. Import from Chrome/Firefox/Bitwarden CSV + export to CSV and JSON

## Architecture

All new logic follows the existing pattern: Rust Tauri commands handle data and
crypto, `dist/app.js` handles UI and calls `invoke()`. No new plugins or build
tools are introduced. The `csv` crate is added for robust CSV parsing.

---

## Feature 1: Edit Credential

### Tauri command

```rust
#[tauri::command]
pub fn update_credential(
    id: String,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    state: State<AppState>,
) -> Result<(), String>
```

Loads `VaultContents`, finds the credential by `id`, updates all fields and
sets `updated_at = now_utc().unix_timestamp()`, then saves back via
`save_contents`. Returns `Err("Credential not found")` if the ID is missing.

### UI

The existing Add modal is reused. When the user clicks an **Edit** button in
the detail panel, the modal opens pre-filled with the credential's current
values. The submit button reads "Save changes" instead of "Save". On success
the detail panel refreshes and the credential list re-renders.

The modal is driven by a `modalMode` state value (`"add" | "edit"`) and an
`editingId` field. No duplicate HTML is needed.

---

## Feature 2: Password Generator

### Tauri command

```rust
#[tauri::command]
pub fn generate_password(
    length: u8,
    upper: bool,
    lower: bool,
    digits: bool,
    symbols: bool,
) -> Result<String, String>
```

Builds a character set string from the enabled categories:
- Uppercase: `ABCDEFGHIJKLMNOPQRSTUVWXYZ`
- Lowercase: `abcdefghijklmnopqrstuvwxyz`
- Digits: `0123456789`
- Symbols: `!@#$%^&*()-_=+[]{}|;:,.<>?`

At least one category must be enabled — returns `Err("no character set selected")`
otherwise. Uses `random_vec` from `espass-crypto-core` for cryptographically
secure bytes with rejection sampling to eliminate modulo bias: for charset size
`n`, bytes ≥ `(256 - 256 % n)` are discarded and resampled. Generates `length`
accepted bytes, maps each to `charset[byte % n]`.

`length` is clamped to `[8, 64]` server-side regardless of UI value.

### UI

A **Generate** button appears next to the password field in both the Add and
Edit modals. Clicking it expands an inline generator panel directly below the
password input:

```
[━━━━━━━━━━━━━━━━━━━━━━━━━━━━━] 20
☑ A–Z  ☑ a–z  ☑ 0–9  ☑ Symbols
[Generate password]
```

- Length slider: 8–64, default 20, value shown as a number beside the slider
- Four checkboxes: uppercase, lowercase, digits, symbols (all on by default)
- At least one checkbox must remain checked (the last one cannot be unchecked)
- Clicking "Generate password" calls `invoke('generate_password', {...})` and
  fills the password `<input>` with the result
- The panel collapses if the user clicks away or closes the modal

---

## Feature 3: Import / Export

### Import

```rust
#[tauri::command]
pub fn import_credentials(
    csv_text: String,
    state: State<AppState>,
) -> Result<ImportSummary, String>

#[derive(serde::Serialize)]
pub struct ImportSummary {
    pub imported: u32,
    pub skipped: u32,   // duplicates (same title + username)
    pub errors: u32,    // rows that could not be parsed
}
```

The `csv` crate parses the text. Format is auto-detected from the header row:

| Browser | Key headers |
|---|---|
| Chrome / Edge | `name, url, username, password` |
| Firefox | `url, username, password, httpRealm` |
| Bitwarden | `login_uri, login_username, login_password, name` |

Rows missing a username AND password are skipped silently. A credential is
considered a duplicate if an existing entry has the same `title` and `username`
(case-insensitive) — duplicates increment `skipped`, not `errors`.

Imported credentials get fresh UUIDs and `created_at = updated_at = now`.

### Export CSV

```rust
#[tauri::command]
pub fn export_credentials_csv(state: State<AppState>) -> Result<String, String>
```

Returns a CSV string (Chrome-compatible headers: `name,url,username,password`)
with all credentials. The vault must be unlocked. JS downloads it as
`espass-export.csv` via a temporary blob URL.

### Export JSON

```rust
#[tauri::command]
pub fn export_credentials_json(state: State<AppState>) -> Result<String, String>
```

Returns a pretty-printed JSON string of `VaultContents` (same structure as the
in-memory vault). JS downloads it as `espass-export.json` via a temporary blob
URL. JSON import is not in scope for this phase — the file serves as a
plaintext backup only.

### UI

A **Tools** button in the topbar opens a small dropdown menu:

```
┌─────────────────┐
│ Import CSV      │
│ Export CSV      │
│ Export JSON     │
└─────────────────┘
```

- **Import CSV**: triggers a hidden `<input type="file" accept=".csv">`, reads
  the file with FileReader, calls `import_credentials(csv_text)`, then shows a
  result toast: *"Imported 42, skipped 3 duplicates"*
- **Export CSV / Export JSON**: shows a warning dialog first — *"This file will
  contain your passwords in plaintext. Store it securely."* — then calls the
  export command and triggers a browser download

---

## Files Changed

| File | Change |
|---|---|
| `apps/desktop/src-tauri/Cargo.toml` | Add `csv = "1.3"` |
| `apps/desktop/src-tauri/src/commands.rs` | Add `update_credential`, `generate_password`, `import_credentials`, `export_credentials_csv`, `export_credentials_json` |
| `apps/desktop/src-tauri/src/lib.rs` | Register the 5 new commands |
| `apps/desktop/dist/app.js` | Edit modal mode, generator panel, tools menu, import/export UI |
| `apps/desktop/dist/style.css` | Styles for generator panel, tools dropdown, toast notification |

No changes to `capabilities/main-window.json` — no new Tauri plugin permissions
needed.

---

## Security Notes

- Password generation uses `random_vec` from `espass-crypto-core` (OS CSPRNG)
- CSV/JSON exports contain plaintext passwords — user is warned before download
- Import runs inside the unlocked vault session — vault must be open
- Duplicate detection is best-effort (title + username match) — not
  cryptographic deduplication
