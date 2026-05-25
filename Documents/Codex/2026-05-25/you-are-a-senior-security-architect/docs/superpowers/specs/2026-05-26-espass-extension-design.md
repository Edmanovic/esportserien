# ESPASS Browser Extension Design

## Goal

Add a Chrome/Chromium browser extension that detects login forms, shows an
inline credential dropdown, and fills username and password fields — all
without the user opening the desktop app. The desktop app runs as a system
tray process; the extension communicates with it via a native messaging host
binary and a local WebSocket IPC server.

## Architecture

```
Chrome page
  └── content script (autofill-guard.ts + dropdown.ts)
        └── background service worker (service-worker.ts)
              └── native messaging stdio
                    └── espass-host binary (stdio ↔ WebSocket bridge)
                          └── ws://127.0.0.1:{port}  (IPC server in Tauri app)
                                └── unlocked vault
```

Three separate processes:
1. **Chrome** — runs content scripts and background service worker
2. **espass-host** — small Rust binary, spawned by Chrome, bridges stdio ↔ WebSocket
3. **Tauri desktop app** — holds the unlocked vault, runs WebSocket IPC server

No vault logic lives in the extension or the native host. Credentials never
touch the extension background context until the user actively selects one.

**Tech stack additions:**
- Desktop: `tokio-tungstenite`, `tauri-plugin-autostart`
- Native host: new Cargo member `apps/desktop/native-messaging-host`
- Extension: `esbuild` to bundle TypeScript, no runtime framework

---

## Desktop Changes

### System Tray (`tray.rs`)

The desktop app minimises to the system tray instead of closing. A tray icon
with a lock/unlock indicator is always visible.

Tray menu:
```
✅ ESPASS               (icon reflects lock state)
────────────────
Åbn
Lås vault
Auto-lock: 15 min ▸    (submenu: 1, 5, 15, 60, 240 min, Aldrig)
────────────────
Afslut
```

On first run the app registers itself in the Windows startup registry:
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run\ESPASS`.
Implemented via `tauri-plugin-autostart`.

### WebSocket IPC Server (`ipc_server.rs`)

Started at app launch and runs continuously — even when the vault is locked.
Binds to `127.0.0.1:0` (OS-assigned random port). Writes the port to
`%APPDATA%/espass/ipc.port` on startup and deletes it on app exit.

Only connections from `127.0.0.1` are accepted. No auth token is needed
because only `espass-host` — which Chrome spawns from the registered manifest
path — can reach this socket on localhost.

**Locked vs unlocked behaviour:**
- When locked: only `unlock` messages are processed. All other message types
  return `{"type":"error","code":"vault-locked"}`.
- When unlocked: all messages are processed normally.

**Messages handled:**

```jsonc
// Find credentials matching an origin
→ { "type": "find_credentials", "origin": "https://github.com" }
← { "type": "credentials", "items": [{ "id": "uuid", "title": "GitHub", "username": "user@example.com" }] }
// Passwords are NOT included in the list response.

// Fetch a single credential after user selection
→ { "type": "get_credential", "id": "uuid" }
← { "type": "credential", "username": "user@example.com", "password": "s3cr3t" }

// Unlock vault from extension popup
→ { "type": "unlock", "password": "master-password" }
← { "type": "unlock_result", "ok": true }
// On failure: { "type": "unlock_result", "ok": false, "reason": "wrong-password" }

// Server pushes when vault locks (e.g. auto-lock fires)
← { "type": "vault_locked" }
```

### Credential Matching

Origin matching uses eTLD+1 comparison. The stored `url` field is parsed to
extract the effective domain. `https://app.github.com` matches a stored URL
of `https://github.com` because both share the eTLD+1 `github.com`. HTTP
origins are never matched — only HTTPS.

### Auto-lock

A timer in `tray.rs` resets on any vault access (find, get, unlock). When it
fires, the vault is locked: `lock_vault()` is called, the WebSocket server
stops, and `ipc.port` is deleted. The tray icon switches to the locked state.

Configurable timeouts: 1, 5, 15, 60, 240 minutes, never. Default: 15 minutes.
The selected timeout is persisted in `sync_state.json` (no secrets).

### New Tauri Commands

```rust
#[tauri::command]
pub fn set_autolock_timeout(minutes: Option<u32>, state: State<AppState>) -> Result<(), String>
// None = never. Restarts the auto-lock timer with the new value.

#[tauri::command]
pub fn get_lock_status(state: State<AppState>) -> LockStatus

pub struct LockStatus {
    pub unlocked: bool,
    pub autolock_minutes: Option<u32>,
    pub ipc_port: Option<u16>,
}
```

---

## Native Messaging Host (`espass-host`)

A standalone Rust binary at `apps/desktop/native-messaging-host/src/main.rs`.
Chrome spawns it on demand and kills it when done. It has no vault logic and
stores no secrets beyond the lifetime of a single forwarded message.

### Startup sequence

1. Read `%APPDATA%/espass/ipc.port`
2. Connect to `ws://127.0.0.1:{port}`
3. If port file missing or connection refused → write
   `{"type":"error","code":"desktop-unavailable"}` to stdout and exit
4. Enter forwarding loop: stdin → WebSocket, WebSocket → stdout

### Message framing

Chrome native messaging uses length-prefixed JSON: 4-byte little-endian
length followed by UTF-8 JSON. The binary reads and writes this format on
stdio and passes raw JSON strings over the WebSocket.

### Registration

The manifest at `apps/desktop/native-messaging-host/manifests/chromium.json`
is written to:
`HKCU\Software\Google\Chrome\NativeMessagingHosts\com.espass.desktop`

The `path` field contains the absolute path to the built `espass-host.exe`.
The `allowed_origins` field contains the packed extension ID set at release
time. The installer handles registration; the manifest in the repo keeps
`REPLACED_BY_INSTALLER` as a placeholder.

### Dependencies (`Cargo.toml`)

```toml
[dependencies]
tokio          = { version = "1", features = ["rt-multi-thread", "io-std", "net"] }
tokio-tungstenite = "0.23"
serde_json     = "1"
```

No crypto-core dependency — the host is a dumb pipe.

---

## Extension

### Manifest (`manifest.json`)

```json
{
  "manifest_version": 3,
  "name": "ESPASS",
  "version": "0.1.0",
  "permissions": ["activeTab", "scripting", "nativeMessaging"],
  "background": { "service_worker": "background.js" },
  "content_scripts": [{
    "matches": ["https://*/*"],
    "js": ["content.js"],
    "run_at": "document_idle"
  }],
  "action": { "default_popup": "popup.html" }
}
```

### Background service worker (`service-worker.ts`)

Owns the native host connection (one port shared across all tabs in the
session). Existing `connectNativeHost()` is extended with:

- Response routing: matches WebSocket responses to pending requests by
  `request_id` (a random UUID added to every outgoing message)
- Credential cache: stores `[{id, title, username}]` per origin in a
  `Map<origin, CachedCredentials>` in memory (session only, no
  `chrome.storage`)
- Vault-locked handler: on `{type:"vault_locked"}` clears the cache and
  broadcasts `{type:"vault_locked"}` to all content scripts

Message protocol between content script and background:

```ts
// Content → Background
{ type: "find_credentials", origin: string }
{ type: "fill_credential",  id: string }
{ type: "get_vault_status" }

// Background → Content
{ type: "credentials",   items: Credential[] }
{ type: "fill_data",     username: string, password: string }
{ type: "vault_status",  state: "ready" | "locked" | "unavailable" }
{ type: "vault_locked" }  // push, no request
```

### Content script — autofill guard (`autofill-guard.ts`)

Existing click listener is extended:

1. On password field click: send `find_credentials` to background
2. On response: if `items.length > 0` render dropdown, else do nothing
3. On `vault_locked` push: dismiss any open dropdown

Field detection: the clicked field must be `type="password"` or have
`autocomplete` containing `current-password` / `new-password`. Plain text
fields are ignored to avoid triggering on search boxes.

After user selects a credential, the script locates the username field by
scanning the same `<form>` for the first visible `type="text"` or
`type="email"` input that appears before the password field. Both fields
receive `.value =` assignment followed by a dispatched `input` event and
`change` event so frameworks (React, Vue, Angular) register the change.

### Content script — dropdown (`dropdown.ts`)

New file. Creates a Shadow DOM host element positioned absolutely below the
clicked field.

```
┌─────────────────────────────────┐
│ 🔑 GitHub                       │
│    user@example.com             │
│ 🔑 GitHub (work)                │
│    nick@company.com             │
└─────────────────────────────────┘
```

- Styled entirely inside Shadow DOM — immune to page CSS
- Keyboard: ↓/↑ navigate, Enter selects, Escape dismisses, Tab dismisses
- Dismissed on `focusout` outside the shadow root or click outside
- At most one dropdown exists at a time — previous is removed before creating a new one

### Extension popup (`popup.html` + `popup.ts`)

Three states rendered in the same popup shell:

**App unavailable:**
```
⚠️  ESPASS desktop kører ikke
    Åbn ESPASS for at bruge autofill
[ Åbn ESPASS ]
```

**Vault locked:**
```
🔒  ESPASS er låst
    Master password
    [••••••••••••••••]
[ Lås op ]            ← calls unlock via background → native host → Tauri
```

**Ready:**
```
✅  ESPASS klar
    Auto-lock: 15 min
[ Lås vault ]
```

"Åbn ESPASS" attempts `chrome.tabs.create({ url: "espass://" })` as a
deep-link. If the OS does not handle the protocol, the popup falls back to
showing the instruction "Åbn ESPASS-appen manuelt".

---

## Build

`package.json` adds an `extension:build` script using `esbuild`:

```json
"extension:build": "esbuild apps/extension/src/background/service-worker.ts apps/extension/src/content/autofill-guard.ts apps/extension/src/popup/popup.ts --bundle --outdir=apps/extension/dist --format=esm --target=chrome120"
```

Output files: `background.js`, `content.js`, `popup.js` in
`apps/extension/dist/`. The native host binary is built separately:

```
cargo build -p espass-host --release
```

---

## Security Notes

- Passwords never enter the extension background context until the user
  actively selects a credential — the `find_credentials` response contains
  only `id`, `title`, and `username`.
- The Shadow DOM dropdown is isolated from page scripts — a malicious page
  cannot read dropdown content via DOM APIs.
- Origin matching requires HTTPS and exact eTLD+1 — HTTP sites and suspicious
  IDN domains (detected by existing `detectSuspiciousDomain`) never trigger autofill.
- Cross-origin iframes never trigger autofill (existing `crossOriginIframe`
  check).
- The WebSocket IPC server binds to `127.0.0.1` only and accepts connections
  only while the vault is unlocked.
- The native host manifest pins the exact extension ID in `allowed_origins` —
  a different extension cannot impersonate ESPASS to the native host.
- Auto-lock (default 15 min) limits the exposure window if the user leaves
  their machine unlocked.

---

## Files Changed

| File | Change |
|------|--------|
| `apps/desktop/src-tauri/src/tray.rs` | New — system tray, autostart, auto-lock timer |
| `apps/desktop/src-tauri/src/ipc_server.rs` | New — WebSocket IPC server, message routing |
| `apps/desktop/src-tauri/src/commands.rs` | Add `set_autolock_timeout`, `get_lock_status` |
| `apps/desktop/src-tauri/src/lib.rs` | Wire tray and IPC server into app startup |
| `apps/desktop/src-tauri/Cargo.toml` | Add `tokio-tungstenite`, `tauri-plugin-autostart` |
| `apps/desktop/native-messaging-host/src/main.rs` | New — stdio↔WebSocket bridge |
| `apps/desktop/native-messaging-host/Cargo.toml` | New — `tokio`, `tokio-tungstenite`, `serde_json` |
| `apps/desktop/native-messaging-host/manifests/chromium.json` | Update path placeholder at build |
| `apps/extension/manifest.json` | New — MV3 manifest |
| `apps/extension/src/background/service-worker.ts` | Extend — response routing, credential cache |
| `apps/extension/src/content/autofill-guard.ts` | Extend — dropdown trigger, field fill |
| `apps/extension/src/content/dropdown.ts` | New — Shadow DOM dropdown component |
| `apps/extension/src/popup/popup.html` | New — toolbar popup HTML shell |
| `apps/extension/src/popup/popup.ts` | New — popup state logic and unlock form |
| `package.json` | Add `extension:build` esbuild script |
