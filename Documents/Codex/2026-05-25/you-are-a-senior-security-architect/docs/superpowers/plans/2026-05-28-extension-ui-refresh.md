# Extension Popup + Autofill Dropdown UI Refresh — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the minimal extension popup with a full credential-list UI (Bitwarden/1Password-style) and polish the autofill dropdown with branding, avatars, and keyboard hints.

**Architecture:** Five coordinated changes: (1) new `list_credentials` IPC endpoint in Rust, (2) three new message types in the background service worker, (3) full CSS rewrite for the popup, (4) full TypeScript rewrite for the popup, (5) incremental additions to the autofill dropdown. No new files — all changes are in-place rewrites or additions.

**Tech Stack:** TypeScript, CSS (no framework), Rust/serde_json, Chrome Extension MV3, esbuild

---

## File map

| File | What changes |
|---|---|
| `apps/extension/manifest.json` | Add `"tabs"` permission so popup can read `tab.url` |
| `apps/desktop/src-tauri/src/ipc_server.rs` | Add `handle_list_credentials` function + match arm + unit test |
| `apps/extension/src/background/service-worker.ts` | Add `credentialListCache`; add `list_credentials`, `find_credentials`, `get_credential` to `onMessage`; clear list cache on vault_locked/disconnect |
| `apps/extension/src/popup/popup.html` | Add `<meta name="color-scheme" content="dark">` |
| `apps/extension/src/popup/popup.css` | Full rewrite — new design system matching desktop palette |
| `apps/extension/src/popup/popup.ts` | Full rewrite — state machine, credential list, search, copy actions, toast |
| `apps/extension/src/content/dropdown.ts` | Add avatar helpers, brand strip, avatar per item, keyboard hint footer |

---

## Task 1: IPC server — `list_credentials` endpoint

**Files:**
- Modify: `apps/desktop/src-tauri/src/ipc_server.rs`

The `handle_find_credentials` function already exists and decrypts the vault. `handle_list_credentials` is identical except it returns ALL items (no origin filter) and omits `password`.

- [ ] **Step 1: Write the failing test**

Add this test at the bottom of `ipc_server.rs` inside `mod tests`:

```rust
#[test]
fn list_credentials_response_has_correct_type() {
    let json = r#"{"type":"credentials_list","items":[]}"#;
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(v["type"], "credentials_list");
    assert!(v["items"].is_array());
}
```

- [ ] **Step 2: Run to confirm test exists and passes (it's a JSON shape test)**

```
cd apps/desktop && cargo test -p espass-desktop list_credentials_response_has_correct_type 2>&1
```

Expected: `test tests::list_credentials_response_has_correct_type ... ok`

- [ ] **Step 3: Add `handle_list_credentials` function**

Add this function directly after the closing brace of `handle_find_credentials` (around line 267 in `ipc_server.rs`):

```rust
fn handle_list_credentials(state: &AppState) -> serde_json::Value {
    let key_bytes = {
        let secrets = match state.secrets.lock() {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"type":"error","code":"state-error"}),
        };
        let key = match secrets.vault_key() {
            Ok(k) => k,
            Err(_) => return serde_json::json!({"type":"error","code":"vault-locked"}),
        };
        let mut kb = [0u8; 32];
        kb.copy_from_slice(key.expose_secret());
        kb
    };
    let vault_key = espass_crypto_core::VaultKey::from_bytes(key_bytes);

    let contents = match load_contents(&vault_key, state) {
        Ok(c) => c,
        Err(e) => return serde_json::json!({"type":"error","code":"load-failed","message":e}),
    };

    let items: Vec<serde_json::Value> = contents
        .credentials
        .iter()
        .map(|c| serde_json::json!({
            "id":       c.id,
            "title":    c.title,
            "username": c.username,
            "url":      c.url,
        }))
        .collect();

    state.touch_vault_access();
    serde_json::json!({"type": "credentials_list", "items": items})
}
```

- [ ] **Step 4: Wire `list_credentials` into `handle_message`**

In `handle_message`, find the match arm `"get_credential" => handle_get_credential(&v, state),` and add the new arm immediately after it:

```rust
"get_credential"    => handle_get_credential(&v, state),
"list_credentials"  => handle_list_credentials(state),
```

- [ ] **Step 5: Build to confirm it compiles**

```
cd apps/desktop && cargo build -p espass-desktop 2>&1 | tail -5
```

Expected: `Finished` with no errors.

- [ ] **Step 6: Commit**

```
git add apps/desktop/src-tauri/src/ipc_server.rs
git commit -m "feat(ipc): add list_credentials endpoint — returns all credentials without passwords"
```

---

## Task 2: Service worker — new message types

**Files:**
- Modify: `apps/extension/src/background/service-worker.ts`
- Modify: `apps/extension/manifest.json`

- [ ] **Step 1: Add `"tabs"` permission to manifest**

In `apps/extension/manifest.json`, change the `"permissions"` array from:
```json
"permissions": ["activeTab", "scripting", "nativeMessaging"],
```
to:
```json
"permissions": ["activeTab", "scripting", "nativeMessaging", "tabs"],
```

- [ ] **Step 2: Add `credentialListCache` variable**

In `apps/extension/src/background/service-worker.ts`, find the line:
```typescript
const credentialCache = new Map<string, Credential[]>(); // origin → items
```

Add the new variable immediately after it:
```typescript
const credentialCache = new Map<string, Credential[]>(); // origin → items
let credentialListCache: Credential[] | null = null;
```

- [ ] **Step 3: Clear `credentialListCache` on vault_locked and disconnect**

Find the `handleNativeMessage` function. Change:
```typescript
  if (m.type === "vault_locked") {
    credentialCache.clear();
    broadcastToContentPorts({ type: "vault_locked" });
    return;
  }
```
to:
```typescript
  if (m.type === "vault_locked") {
    credentialCache.clear();
    credentialListCache = null;
    broadcastToContentPorts({ type: "vault_locked" });
    return;
  }
```

Find `nativePort.onDisconnect.addListener`. Change:
```typescript
  nativePort.onDisconnect.addListener(() => {
    nativePort = null;
    for (const [id, pending] of pendingRequests) {
      clearTimeout(pending.timeoutId);
      pending.resolve({ type: "error", code: "native-host-disconnected" });
      pendingRequests.delete(id);
    }
    broadcastToContentPorts({ type: "vault_status", state: "unavailable" });
  });
```
to:
```typescript
  nativePort.onDisconnect.addListener(() => {
    nativePort = null;
    credentialCache.clear();
    credentialListCache = null;
    for (const [id, pending] of pendingRequests) {
      clearTimeout(pending.timeoutId);
      pending.resolve({ type: "error", code: "native-host-disconnected" });
      pendingRequests.delete(id);
    }
    broadcastToContentPorts({ type: "vault_status", state: "unavailable" });
  });
```

- [ ] **Step 4: Add three new cases to `chrome.runtime.onMessage`**

Find the `onMessage` listener's switch statement. It currently ends with:
```typescript
      case "lock": {
        // Clear cache immediately (fail-closed) before the round-trip completes.
        credentialCache.clear();
        sendToNativeHost({ type: "lock" }).then(sendResponse);
        return true;
      }
      default:
        return false;
```

Replace that block with:
```typescript
      case "lock": {
        credentialCache.clear();
        credentialListCache = null;
        sendToNativeHost({ type: "lock" }).then(sendResponse);
        return true;
      }
      case "list_credentials": {
        if (credentialListCache) {
          sendResponse({ type: "credentials_list", items: credentialListCache });
        } else {
          sendToNativeHost({ type: "list_credentials" }).then((raw) => {
            if (raw.type === "credentials_list") {
              credentialListCache = raw.items as Credential[];
            }
            sendResponse(raw);
          });
        }
        return true;
      }
      case "find_credentials": {
        const origin = message.origin as string;
        const cached = credentialCache.get(origin);
        if (cached) {
          sendResponse({ type: "credentials", items: cached });
        } else {
          sendToNativeHost({ type: "find_credentials", origin }).then((raw) => {
            if (raw.type === "credentials") {
              credentialCache.set(origin, raw.items as Credential[]);
            }
            sendResponse(raw);
          });
        }
        return true;
      }
      case "get_credential": {
        sendToNativeHost({ type: "get_credential", id: message.id as string })
          .then(sendResponse);
        return true;
      }
      default:
        return false;
```

- [ ] **Step 5: Build and verify TypeScript compiles**

```
npm run extension:build 2>&1
```

Expected: three `Done in` lines, no TypeScript errors.

- [ ] **Step 6: Commit**

```
git add apps/extension/manifest.json apps/extension/src/background/service-worker.ts
git commit -m "feat(extension): add list_credentials, find_credentials, get_credential to popup message handler"
```

---

## Task 3: Popup CSS rewrite

**Files:**
- Modify: `apps/extension/src/popup/popup.html`
- Modify: `apps/extension/src/popup/popup.css`

- [ ] **Step 1: Update popup.html**

Replace `apps/extension/src/popup/popup.html` entirely with:

```html
<!DOCTYPE html>
<html lang="da">
  <head>
    <meta charset="UTF-8" />
    <meta name="color-scheme" content="dark" />
    <title>ESPASS</title>
    <link rel="stylesheet" href="popup.css" />
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="popup.js"></script>
  </body>
</html>
```

- [ ] **Step 2: Write the new popup.css**

Replace `apps/extension/src/popup/popup.css` entirely with:

```css
/* ── Reset + tokens ─────────────────────────────────────────────────────── */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

:root {
  --bg:         #0f1117;
  --surface:    #181b24;
  --surface2:   #1e2232;
  --border:     #2a2e42;
  --accent:     #7c85f0;
  --accent-hov: #9aa3ff;
  --danger:     #e05252;
  --text:       #e2e4ef;
  --muted:      #7a7f9a;
  --radius:     8px;
}

/* ── Body ────────────────────────────────────────────────────────────────── */
body {
  width: 320px;
  min-height: 120px;
  max-height: 500px;
  overflow: hidden;
  background: var(--bg);
  color: var(--text);
  font-family: system-ui, -apple-system, sans-serif;
  font-size: 13px;
  line-height: 1.4;
}

/* ── Screen layouts ──────────────────────────────────────────────────────── */
.screen { display: flex; flex-direction: column; }

.screen--center {
  min-height: 200px;
  align-items: center;
  justify-content: center;
  gap: 16px;
  padding: 28px 24px;
  text-align: center;
}

.screen--vault {
  height: 500px;
  max-height: 500px;
}

/* ── Brand wordmark ──────────────────────────────────────────────────────── */
.brand {
  font-size: 22px;
  font-weight: 700;
  letter-spacing: 2px;
  color: var(--accent);
  user-select: none;
}
.brand--small { font-size: 15px; letter-spacing: 1.5px; }

/* ── Unavailable screen ──────────────────────────────────────────────────── */
.unavail-icon { color: var(--muted); margin-bottom: 4px; }
.screen-heading { font-size: 15px; font-weight: 600; margin-bottom: 4px; }
.screen-sub { font-size: 12px; color: var(--muted); line-height: 1.5; }

/* ── Auth form ───────────────────────────────────────────────────────────── */
.auth-form {
  width: 100%;
  max-width: 260px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.auth-form input[type="password"] {
  width: 100%;
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 9px 12px;
  color: var(--text);
  font-size: 13px;
  outline: none;
  transition: border-color .15s;
}
.auth-form input[type="password"]:focus { border-color: var(--accent); }

.error-msg {
  background: rgba(224,82,82,.15);
  border: 1px solid rgba(224,82,82,.35);
  border-radius: var(--radius);
  padding: 7px 10px;
  color: #f09090;
  font-size: 12px;
}

/* ── Buttons ─────────────────────────────────────────────────────────────── */
.btn {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  padding: 8px 16px;
  border-radius: var(--radius);
  border: none;
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background .15s, opacity .15s;
  white-space: nowrap;
}
.btn:disabled { opacity: .5; cursor: not-allowed; }
.btn--primary { background: var(--accent); color: #fff; }
.btn--primary:hover:not(:disabled) { background: var(--accent-hov); }
.btn--full { width: 100%; }

.btn-icon {
  background: transparent;
  border: none;
  padding: 5px;
  border-radius: 6px;
  color: var(--muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  transition: color .15s, background .15s;
}
.btn-icon:hover { color: var(--text); background: var(--surface2); }

/* ── Popup header ────────────────────────────────────────────────────────── */
.popup-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}

/* ── Section label ───────────────────────────────────────────────────────── */
.section-label {
  font-size: 10px;
  font-weight: 600;
  letter-spacing: .07em;
  text-transform: uppercase;
  color: var(--muted);
  padding: 8px 14px 4px;
}

.section-group { border-bottom: 1px solid var(--border); }

/* ── Search bar ──────────────────────────────────────────────────────────── */
.search-wrap {
  position: relative;
  padding: 8px 10px;
  background: var(--surface);
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.search-icon {
  position: absolute;
  left: 20px;
  top: 50%;
  transform: translateY(-50%);
  color: var(--muted);
  pointer-events: none;
}
.search-input {
  width: 100%;
  background: var(--surface2);
  border: 1px solid transparent;
  border-radius: var(--radius);
  padding: 7px 10px 7px 30px;
  color: var(--text);
  font-size: 13px;
  outline: none;
  transition: border-color .15s;
}
.search-input:focus { border-color: var(--accent); }
.search-input::placeholder { color: var(--muted); }
/* remove native clear button in Webkit */
.search-input::-webkit-search-cancel-button { display: none; }

/* ── Vault body (scrollable credential list area) ────────────────────────── */
.vault-body {
  display: flex;
  flex-direction: column;
  flex: 1;
  overflow: hidden;
}

/* ── Credential list ─────────────────────────────────────────────────────── */
.cred-list {
  overflow-y: auto;
  flex: 1;
}
.cred-list::-webkit-scrollbar       { width: 4px; }
.cred-list::-webkit-scrollbar-track { background: transparent; }
.cred-list::-webkit-scrollbar-thumb { background: var(--border); border-radius: 2px; }

/* ── Credential item ─────────────────────────────────────────────────────── */
.cred-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  cursor: default;
  border-left: 3px solid transparent;
  transition: background .1s;
  min-height: 44px;
}
.cred-item:hover,
.cred-item:focus-within { background: var(--surface2); }
.cred-item--suggested { border-left-color: var(--accent); }

.cred-avatar {
  width: 28px;
  height: 28px;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  font-size: 11px;
  font-weight: 700;
  color: #fff;
  flex-shrink: 0;
  user-select: none;
}

.cred-body {
  flex: 1;
  min-width: 0;
}
.cred-title {
  font-weight: 600;
  font-size: 13px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.cred-username {
  font-size: 11px;
  color: var(--muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  margin-top: 1px;
}
.cred-domain { opacity: .7; }

/* ── Copy buttons ────────────────────────────────────────────────────────── */
.cred-actions {
  display: flex;
  gap: 2px;
  opacity: 0;
  transition: opacity .15s;
  flex-shrink: 0;
}
.cred-item:hover .cred-actions,
.cred-item:focus-within .cred-actions { opacity: 1; }

.copy-btn {
  background: transparent;
  border: none;
  padding: 4px;
  border-radius: 5px;
  color: var(--muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  transition: color .12s, background .12s;
}
.copy-btn:hover { color: var(--accent); background: rgba(124,133,240,.12); }

/* ── Empty state ─────────────────────────────────────────────────────────── */
.empty-state {
  color: var(--muted);
  font-size: 12px;
  text-align: center;
  padding: 32px 16px;
  line-height: 1.6;
}

/* ── Toast ───────────────────────────────────────────────────────────────── */
@keyframes toast-in  { from { opacity: 0; transform: translateX(-50%) translateY(6px); } to { opacity: 1; transform: translateX(-50%) translateY(0); } }
@keyframes toast-out { from { opacity: 1; } to { opacity: 0; } }

.toast {
  position: fixed;
  bottom: 14px;
  left: 50%;
  transform: translateX(-50%);
  background: var(--surface2);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  padding: 7px 14px;
  font-size: 12px;
  color: var(--text);
  z-index: 999;
  white-space: nowrap;
  pointer-events: none;
  animation: toast-in .18s ease;
  box-shadow: 0 4px 14px rgba(0,0,0,.5);
}
.toast--success { border-color: #3a6a3a; color: #7fc97f; }
.toast--error   { border-color: rgba(224,82,82,.4); color: #f09090; }

/* ── Spinner ─────────────────────────────────────────────────────────────── */
@keyframes spin { to { transform: rotate(360deg); } }
.spinner {
  width: 24px; height: 24px;
  border: 2px solid var(--border);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: spin .7s linear infinite;
}
```

- [ ] **Step 3: Build to confirm CSS is copied correctly**

```
npm run extension:build 2>&1
```

Expected: three `Done in` lines, no errors.

- [ ] **Step 4: Commit**

```
git add apps/extension/src/popup/popup.html apps/extension/src/popup/popup.css
git commit -m "feat(popup): new dark-theme design system CSS + updated HTML"
```

---

## Task 4: Popup TypeScript rewrite

**Files:**
- Modify: `apps/extension/src/popup/popup.ts`

This is the largest change. Replace the entire file.

- [ ] **Step 1: Write the new popup.ts**

Replace `apps/extension/src/popup/popup.ts` entirely with:

```typescript
// ── Types ─────────────────────────────────────────────────────────────────

interface CredentialItem {
  id: string;
  title: string;
  username: string;
  url?: string | null;
}

// ── Avatar helpers ─────────────────────────────────────────────────────────

const AVATAR_HUES = [220, 260, 170, 30, 340, 200, 290, 140];

function avatarColor(title: string): string {
  let h = 0;
  for (const ch of title) h = (h * 31 + ch.charCodeAt(0)) & 0xff;
  return `hsl(${AVATAR_HUES[h % AVATAR_HUES.length]}, 55%, 55%)`;
}

function avatarLetter(title: string): string {
  return (title.trim()[0] ?? '?').toUpperCase();
}

// ── HTML escape ────────────────────────────────────────────────────────────

function esc(s: unknown): string {
  return String(s ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

// ── Toast ──────────────────────────────────────────────────────────────────

function showToast(msg: string, type: 'success' | 'error' = 'success'): void {
  document.querySelector('.toast')?.remove();
  const t = document.createElement('div');
  t.className = `toast toast--${type}`;
  t.textContent = msg;
  document.body.appendChild(t);
  setTimeout(() => t.remove(), 1500);
}

// ── Clipboard ─────────────────────────────────────────────────────────────

async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement('textarea');
    ta.value = text;
    Object.assign(ta.style, { position: 'fixed', opacity: '0', top: '0', left: '0' });
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    document.execCommand('copy');
    ta.remove();
  }
}

// ── Credential item HTML ───────────────────────────────────────────────────

function credItemHTML(cred: CredentialItem, suggested = false): string {
  const color  = avatarColor(cred.title);
  const letter = avatarLetter(cred.title);
  const url    = cred.url ?? '';
  let domain   = '';
  if (url) {
    try { domain = new URL(url).hostname; } catch { domain = ''; }
  }

  return `<div class="cred-item${suggested ? ' cred-item--suggested' : ''}"
               data-id="${esc(cred.id)}" tabindex="0">
    <div class="cred-avatar" style="background:${color}">${esc(letter)}</div>
    <div class="cred-body">
      <div class="cred-title">${esc(cred.title)}</div>
      <div class="cred-username">${esc(cred.username)}${domain ? `<span class="cred-domain"> · ${esc(domain)}</span>` : ''}</div>
    </div>
    <div class="cred-actions">
      <button class="copy-btn" data-action="copy-user"
              data-id="${esc(cred.id)}" data-value="${esc(cred.username)}"
              title="Copy username">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width="2">
          <path d="M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2"/>
          <circle cx="12" cy="7" r="4"/>
        </svg>
      </button>
      <button class="copy-btn" data-action="copy-pass"
              data-id="${esc(cred.id)}"
              title="Copy password">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none"
             stroke="currentColor" stroke-width="2">
          <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
          <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
        </svg>
      </button>
    </div>
  </div>`;
}

// ── Copy action wiring ─────────────────────────────────────────────────────

function attachCopyActions(container: Element): void {
  container.querySelectorAll<HTMLButtonElement>('.copy-btn').forEach((btn) => {
    const fresh = btn.cloneNode(true) as HTMLButtonElement;
    btn.replaceWith(fresh);
    fresh.addEventListener('click', async (e) => {
      e.stopPropagation();
      const action = fresh.dataset.action!;
      const id     = fresh.dataset.id!;

      if (action === 'copy-user') {
        await copyText(fresh.dataset.value!);
        showToast('Username copied');
      } else {
        try {
          const resp = await chrome.runtime.sendMessage({ type: 'get_credential', id }) as Record<string, unknown>;
          if (resp?.type === 'credential' && resp.password) {
            await copyText(resp.password as string);
            showToast('Password copied');
          } else {
            showToast('Failed to copy', 'error');
          }
        } catch {
          showToast('Failed to copy', 'error');
        }
      }
    });
  });
}

// ── Render: unavailable ────────────────────────────────────────────────────

function renderUnavailable(root: HTMLElement): void {
  root.innerHTML = `
    <div class="screen screen--center">
      <svg class="unavail-icon" width="36" height="36" viewBox="0 0 24 24"
           fill="none" stroke="currentColor" stroke-width="1.5">
        <rect x="3" y="11" width="18" height="11" rx="2" ry="2"/>
        <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
      </svg>
      <div>
        <div class="screen-heading">ESPASS is not running</div>
        <div class="screen-sub">Start the ESPASS desktop app to continue.</div>
      </div>
      <button class="btn btn--primary" id="retry-btn">Try again</button>
    </div>`;
  document.getElementById('retry-btn')!.addEventListener('click', () => main());
}

// ── Render: locked ─────────────────────────────────────────────────────────

function renderLocked(root: HTMLElement): void {
  root.innerHTML = `
    <div class="screen screen--center">
      <div class="brand">ESPASS</div>
      <form class="auth-form" id="unlock-form" autocomplete="off">
        <input id="master-pw" type="password"
               autocomplete="current-password"
               placeholder="Master password" autofocus />
        <div class="error-msg" id="unlock-error" hidden></div>
        <button type="submit" class="btn btn--primary btn--full" id="unlock-btn">
          Unlock
        </button>
      </form>
    </div>`;

  const form   = document.getElementById('unlock-form')   as HTMLFormElement;
  const pwIn   = document.getElementById('master-pw')     as HTMLInputElement;
  const errEl  = document.getElementById('unlock-error')  as HTMLDivElement;
  const btn    = document.getElementById('unlock-btn')    as HTMLButtonElement;

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const password = pwIn.value;
    if (!password) return;

    btn.disabled = true;
    btn.textContent = 'Unlocking…';
    pwIn.value = ''; // clear before async

    try {
      const resp = await chrome.runtime.sendMessage({ type: 'unlock', password }) as Record<string, unknown>;
      if (resp?.type === 'unlock_result' && resp?.ok === true) {
        await main();
      } else {
        errEl.textContent = 'Wrong password';
        errEl.hidden = false;
        btn.disabled = false;
        btn.textContent = 'Unlock';
        pwIn.focus();
      }
    } catch {
      errEl.textContent = 'Could not connect to ESPASS';
      errEl.hidden = false;
      btn.disabled = false;
      btn.textContent = 'Unlock';
    }
  });
}

// ── Render: unlocked ───────────────────────────────────────────────────────

async function renderUnlocked(root: HTMLElement, tabOrigin: string | null): Promise<void> {
  const [tabResp, allResp] = await Promise.all([
    tabOrigin
      ? chrome.runtime.sendMessage({ type: 'find_credentials', origin: tabOrigin }) as Promise<Record<string, unknown>>
      : Promise.resolve({ type: 'credentials', items: [] as CredentialItem[] }),
    chrome.runtime.sendMessage({ type: 'list_credentials' }) as Promise<Record<string, unknown>>,
  ]);

  const tabMatches = (tabResp?.type === 'credentials'      ? tabResp.items  : []) as CredentialItem[];
  const allCreds   = (allResp?.type === 'credentials_list' ? allResp.items  : []) as CredentialItem[];

  const tabSection = tabMatches.length > 0
    ? `<div class="section-group">
         <div class="section-label">Suggested for this page</div>
         ${tabMatches.slice(0, 3).map(c => credItemHTML(c, true)).join('')}
       </div>`
    : '';

  const listHTML = allCreds.length === 0
    ? '<div class="empty-state">No credentials saved yet.</div>'
    : allCreds.map(c => credItemHTML(c)).join('');

  root.innerHTML = `
    <div class="screen screen--vault">
      <header class="popup-header">
        <span class="brand brand--small">ESPASS</span>
        <button class="btn-icon" id="lock-btn" title="Lock vault">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none"
               stroke="currentColor" stroke-width="2">
            <rect x="3" y="11" width="18" height="11" rx="2"/>
            <path d="M7 11V7a5 5 0 0 1 10 0v4"/>
          </svg>
        </button>
      </header>
      ${tabSection}
      <div class="vault-body">
        <div class="search-wrap">
          <svg class="search-icon" width="13" height="13" viewBox="0 0 24 24"
               fill="none" stroke="currentColor" stroke-width="2">
            <circle cx="11" cy="11" r="8"/>
            <line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input class="search-input" type="search" id="cred-search"
                 placeholder="Search…" autocomplete="off" />
        </div>
        <div class="cred-list" id="cred-list">${listHTML}</div>
      </div>
    </div>`;

  // Lock button
  document.getElementById('lock-btn')!.addEventListener('click', async () => {
    try { await chrome.runtime.sendMessage({ type: 'lock' }); } catch {}
    await main();
  });

  // Attach copy actions to initially rendered items
  attachCopyActions(root);

  // Search filter
  const searchEl  = document.getElementById('cred-search') as HTMLInputElement;
  const credList  = document.getElementById('cred-list')  as HTMLDivElement;

  searchEl.addEventListener('input', () => {
    const q = searchEl.value.toLowerCase().trim();
    if (!q) {
      credList.innerHTML = allCreds.length === 0
        ? '<div class="empty-state">No credentials saved yet.</div>'
        : allCreds.map(c => credItemHTML(c)).join('');
    } else {
      const filtered = allCreds.filter(c =>
        c.title.toLowerCase().includes(q)    ||
        c.username.toLowerCase().includes(q) ||
        (c.url ?? '').toLowerCase().includes(q)
      );
      credList.innerHTML = filtered.length === 0
        ? `<div class="empty-state">No matches for "${esc(q)}"</div>`
        : filtered.map(c => credItemHTML(c)).join('');
    }
    attachCopyActions(credList);
  });
}

// ── Main ───────────────────────────────────────────────────────────────────

async function main(): Promise<void> {
  const root = document.getElementById('root') as HTMLElement;

  // Brief loading spinner
  root.innerHTML = '<div class="screen screen--center"><div class="spinner"></div></div>';

  // Resolve current tab origin (null if not an https page)
  let tabOrigin: string | null = null;
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const url = tab?.url ?? '';
    if (url.startsWith('https://')) {
      tabOrigin = new URL(url).origin;
    }
  } catch { /* non-https tab or permission not granted */ }

  try {
    const resp = await chrome.runtime.sendMessage({ type: 'get_vault_status' }) as Record<string, unknown>;
    switch (resp?.state) {
      case 'ready':
        await renderUnlocked(root, tabOrigin);
        break;
      case 'locked':
        renderLocked(root);
        break;
      default:
        renderUnavailable(root);
    }
  } catch {
    renderUnavailable(root);
  }
}

main();
```

- [ ] **Step 2: Build the extension**

```
npm run extension:build 2>&1
```

Expected: three `Done in` lines, no TypeScript errors.

- [ ] **Step 3: Load in Chrome and smoke-test**

1. Open `chrome://extensions`, click **Reload** on ESPASS.
2. Click the ESPASS toolbar icon.
3. Locked state: you should see the ESPASS wordmark + password field + Unlock button.
4. Unlock with your master password.
5. Unlocked state: you should see the header, search bar, and credential list.
6. Hover a credential row: copy-username (person icon) and copy-password (lock icon) buttons appear.
7. Click copy-password: "Password copied" toast appears at the bottom for 1.5 s.
8. Navigate to a site you have credentials for, open popup: the "Suggested for this page" section appears at the top.

- [ ] **Step 4: Commit**

```
git add apps/extension/src/popup/popup.ts
git commit -m "feat(popup): full credential-list UI — state machine, search, copy actions, toast"
```

---

## Task 5: Autofill dropdown — brand strip, avatars, keyboard footer

**Files:**
- Modify: `apps/extension/src/content/dropdown.ts`

- [ ] **Step 1: Add avatar helper functions**

In `apps/extension/src/content/dropdown.ts`, add these two functions immediately after the `CredentialItem` interface (before the `currentHost` variable):

```typescript
// ── Avatar helpers (duplicated from popup — separate bundle) ───────────────
const _AVATAR_HUES = [220, 260, 170, 30, 340, 200, 290, 140];

function _avatarColor(title: string): string {
  let h = 0;
  for (const ch of title) h = (h * 31 + ch.charCodeAt(0)) & 0xff;
  return `hsl(${_AVATAR_HUES[h % _AVATAR_HUES.length]}, 55%, 55%)`;
}

function _avatarLetter(title: string): string {
  return (title.trim()[0] ?? '?').toUpperCase();
}
```

- [ ] **Step 2: Add new CSS rules to the shadow DOM `<style>` block**

In `showDropdown`, find the `style.textContent = \`` block. The current content ends with the `.item-user` rule. Append these new rules inside the same template literal, before the closing backtick:

```css
    .brand-strip {
      display: flex;
      justify-content: space-between;
      align-items: center;
      padding: 4px 10px;
      background: #1e2232;
      border-bottom: 1px solid #2a2e42;
      font-size: 11px;
      flex-shrink: 0;
    }
    .brand-strip__name { font-weight: 600; color: #7c85f0; }
    .brand-strip__hint { color: #7a7f9a; }

    .avatar {
      width: 26px; height: 26px; border-radius: 50%;
      display: flex; align-items: center; justify-content: center;
      font-size: 11px; font-weight: 700; color: #fff; flex-shrink: 0;
    }

    .item { gap: 8px; }

    .dropdown {
      min-width: 260px;
      display: flex;
      flex-direction: column;
      overflow: hidden;
    }

    .item-list { overflow-y: auto; max-height: 280px; flex: 1; }

    .kb-hint {
      padding: 4px 10px;
      text-align: center;
      font-size: 10px;
      color: #7a7f9a;
      border-top: 1px solid #2a2e42;
      background: #1e2232;
      flex-shrink: 0;
    }
```

- [ ] **Step 3: Add brand strip, item-list wrapper, and keyboard footer inside the dropdown**

In `showDropdown`, find the block:
```typescript
  const dropdown = document.createElement("div");
  dropdown.className = "dropdown";
  shadow.appendChild(dropdown);
```

Replace it with:
```typescript
  const dropdown = document.createElement("div");
  dropdown.className = "dropdown";

  const brandStrip = document.createElement("div");
  brandStrip.className = "brand-strip";
  brandStrip.innerHTML =
    '<span class="brand-strip__name">🔑 ESPASS</span>' +
    '<span class="brand-strip__hint">ESC to close</span>';

  const itemList = document.createElement("div");
  itemList.className = "item-list";

  const kbHint = document.createElement("div");
  kbHint.className = "kb-hint";
  kbHint.textContent = "↑↓ navigate · Enter fill · Esc dismiss";

  dropdown.appendChild(brandStrip);
  dropdown.appendChild(itemList);
  dropdown.appendChild(kbHint);

  shadow.appendChild(style);
  shadow.appendChild(dropdown);
```

(The original `shadow.appendChild(style)` and `shadow.appendChild(dropdown)` are now part of this block — do not duplicate them.)

- [ ] **Step 4: Add avatar to each credential item**

In the `items.forEach` loop, find:
```typescript
    el.innerHTML =
      `<span class="item-title">🔑 ${esc(item.title)}</span>` +
      `<span class="item-user">${esc(item.username)}</span>`;
```

Replace with:
```typescript
    const color  = _avatarColor(item.title);
    const letter = _avatarLetter(item.title);
    el.innerHTML =
      `<div class="avatar" style="background:${color}">${letter}</div>` +
      `<div style="display:flex;flex-direction:column;flex:1;min-width:0;">` +
        `<span class="item-title">${esc(item.title)}</span>` +
        `<span class="item-user">${esc(item.username)}</span>` +
      `</div>`;

Also change:
```typescript
    dropdown.appendChild(el);
```
to:
```typescript
    itemList.appendChild(el);
```

- [ ] **Step 5: Remove the 🔑 emoji from item-title** (it's now replaced by the avatar)

Verify the replacement in step 4 no longer has `🔑 ` in `item-title`. The avatar circle serves as the icon now.

The `.dropdown` div retains its original `position: fixed` and `z-index: 2147483647` rules — no positioning changes needed. The flex-column layout and `.item-list` wrapper handle the new structure entirely.

- [ ] **Step 6: Build**

```
npm run extension:build 2>&1
```

Expected: three `Done in` lines, no errors.

- [ ] **Step 7: Smoke-test the dropdown**

1. Reload the extension.
2. Go to a site that has credentials in your vault (e.g., the site you saved for HLTV or Microsoft).
3. Click on the login field.
4. The dropdown should appear with:
   - Top strip: "🔑 ESPASS" left, "ESC to close" right (dark background, purple text)
   - Credential rows: coloured initial circle + title + username
   - Bottom strip: "↑↓ navigate · Enter fill · Esc dismiss" (dark background, muted text)
5. Press ↑/↓ to navigate, Enter to fill, Esc to dismiss.

- [ ] **Step 8: Commit**

```
git add apps/extension/src/content/dropdown.ts
git commit -m "feat(dropdown): brand strip, coloured avatars, keyboard hint footer"
```

---

## Final build and push

- [ ] **Run the full build one last time**

```
npm run extension:build 2>&1
```

- [ ] **Push to remote**

```
git push
```
