# ESPASS Extension Popup + Autofill Dropdown — UI Refresh

## Goal

Replace the minimal 3-state extension popup skeleton with a full Bitwarden/1Password-style credential-manager popup, and polish the autofill dropdown with branding, avatars, and keyboard hints.

## Architecture overview

Four files change significantly; one new IPC endpoint is added:

| File | Change |
|---|---|
| `apps/extension/src/popup/popup.css` | Full rewrite — new design system |
| `apps/extension/src/popup/popup.ts` | Full rewrite — new state machine + unlocked credential list |
| `apps/extension/src/popup/popup.html` | Minor — add `<meta name="color-scheme">` |
| `apps/extension/src/content/dropdown.ts` | Add brand strip, avatar, keyboard footer |
| `apps/extension/src/background/service-worker.ts` | Add `list_credentials` + `find_credentials` + `get_credential` to `onMessage` handler; add `credentialListCache` |
| `apps/desktop/src-tauri/src/ipc_server.rs` | Add `list_credentials` handler + unit test |

---

## Design tokens

The popup reuses the desktop app's palette exactly so the two surfaces feel like one product.

```css
:root {
  --bg:          #0f1117;
  --surface:     #181b24;
  --surface2:    #1e2232;
  --border:      #2a2e42;
  --accent:      #7c85f0;
  --accent-hov:  #9aa3ff;
  --danger:      #e05252;
  --text:        #e2e4ef;
  --muted:       #7a7f9a;
  --radius:      8px;
}
```

**Popup dimensions:** 320 × auto (max-height 500 px, credential list scrolls independently).

**Credential avatar:** 28 × 28 px circle. Background colour derived deterministically from the credential title (hash → one of 8 hues). Letter = `title[0].toUpperCase()`. Falls back to `?` if title is empty.

```typescript
const AVATAR_HUES = [220, 260, 170, 30, 340, 200, 290, 140];

function avatarColor(title: string): string {
  let h = 0;
  for (const ch of title) h = (h * 31 + ch.charCodeAt(0)) & 0xff;
  return `hsl(${AVATAR_HUES[h % AVATAR_HUES.length]}, 55%, 55%)`;
}

function avatarLetter(title: string): string {
  return (title.trim()[0] ?? '?').toUpperCase();
}
```

---

## Extension popup — state machine

```
main()
  ├─ chrome.tabs.query(active)          → tabOrigin (or null for non-http tabs)
  ├─ sendMessage(get_vault_status)      → state
  │
  ├─ state === "unavailable"  → renderUnavailable()
  ├─ state === "locked"       → renderLocked()
  └─ state === "ready"        → renderUnlocked(tabOrigin)
       ├─ if tabOrigin is non-null (https:// tab):
       │    Promise.all([
       │      sendMessage(find_credentials, origin: tabOrigin),
       │      sendMessage(list_credentials)
       │    ])
       │  else (chrome://, about:, file://, etc.):
       │    Promise.all([
       │      Promise.resolve({ type: "credentials", items: [] }),
       │      sendMessage(list_credentials)
       │    ])
       └─ render header + tab section (if tabMatches.length > 0) + search + all-list
```

### renderUnavailable()

Centered layout:
- Lock icon (SVG, 32 px, `--muted` colour)
- Heading: "ESPASS is not running"
- Sub-text: "Start the ESPASS desktop app to continue."
- Primary button: "Try again" → calls `main()`

### renderLocked()

Centered layout:
- ESPASS wordmark (`--accent`, 22 px, letter-spacing 2 px)
- Password input (`autocomplete="current-password"`, `autofocus`)
- Primary button: "Unlock" (full width)
- Error message area below button (hidden until needed)
- Password is cleared from DOM immediately before the async `sendMessage` call

### renderUnlocked(tabOrigin, tabMatches, allCredentials)

Three-zone layout (no scrollbar visible on the header):

```
┌──────────────────────────────────┐
│ ESPASS                      🔒   │  ← header (sticky)
├──────────────────────────────────┤
│ [current tab section]            │  ← only shown when tabMatches.length > 0
├──────────────────────────────────┤
│ 🔍 Search…                       │  ← search input
│ ─────────────────────────────── │
│  A  Amazon          user@…  👤🔑 │  ← credential items (scrollable)
│  G  GitHub          nick@…  👤🔑 │
│  …                               │
└──────────────────────────────────┘
```

**Header:** flex row, `--surface` background, 1 px bottom border.
- Left: "ESPASS" wordmark
- Right: lock SVG icon button (calls `lock` → `main()`)

**Current tab section** (shown only when `tabMatches.length > 0`):
- Section label: "Suggested for this page" (12 px, `--muted`, uppercase, letter-spacing)
- Up to 3 credential rows, each with a subtle `--accent` left border to distinguish from the full list

**Search input:** `type="search"`, full width, `--surface2` background. Filters the all-list in real time (case-insensitive match against title + username + url). Does NOT filter the tab section.

**Credential item row:**
```
[avatar] [title (bold)]          [copy-user btn] [copy-pw btn]
         [username (muted, small)]
```
- Height: 44 px
- Avatar: 28 × 28 circle
- Copy buttons hidden until row is hovered; show on focus too (keyboard-accessible)
- Copy-user button: copies `username` directly (already in the list payload)
- Copy-password button: calls `sendMessage({ type: "get_credential", id })` → copies `password` → shows "Copied!" toast for 1.5 s, button returns to icon

**Empty state (no credentials at all):** Lock icon + "No credentials saved yet." in muted text, centred in the list area.

**Empty state (search has no results):** "No matches for «query»" in muted text.

---

## popup.css — structure

Sections (all new):

1. **Reset + variables** — same tokens as desktop
2. **Body** — `width: 320px; max-height: 500px; overflow: hidden; background: var(--bg); color: var(--text);`
3. **Screen layouts** — `.screen--center` (flex column centred, used for locked/unavailable), `.screen--vault` (flex column, fills height)
4. **Brand wordmark** — `.brand` (accent colour, bold, letter-spacing)
5. **Auth form** — password input + button + error
6. **Header bar** — `.popup-header` (flex row, surface bg, border-bottom)
7. **Section label** — `.section-label` (uppercase, muted, 11 px)
8. **Search bar** — `.search-wrap input` (surface2 bg, no border visible when unfocused, accent border on focus)
9. **Credential list** — `.cred-list` (overflow-y: auto, max-height ~280 px)
10. **Credential item** — `.cred-item` (flex row, hover bg, 44 px min-height)
11. **Avatar** — `.cred-avatar` (28 px circle, centred letter, white text)
12. **Copy buttons** — `.copy-btn` (icon buttons, hidden by default, visible on `.cred-item:hover` / `.cred-item:focus-within`)
13. **Toast** — `.toast` (fixed bottom-center, fade in/out, success/error variants)
14. **Spinner** — keyframe spin, 24 px circle

---

## Autofill dropdown — changes (dropdown.ts)

Three additions to the existing Shadow DOM component:

### 1. Brand strip (top of dropdown)
```html
<div class="brand-strip">
  <span class="brand-strip__name">🔑 ESPASS</span>
  <span class="brand-strip__hint">ESC to close</span>
</div>
```
Height 24 px, `--surface2` background, bottom border, 10 px horizontal padding.

### 2. Avatar per item
Each `.item` gains a `<div class="avatar">` as first child:
```html
<div class="avatar" style="background: hsl(220,55%,55%)">A</div>
```
Same `avatarColor` / `avatarLetter` functions duplicated in this file (they're tiny — no shared import needed between the two separate bundles).

### 3. Keyboard hint footer (bottom of dropdown)
```html
<div class="kb-hint">↑↓ navigate · Enter fill · Esc dismiss</div>
```
Height 22 px, centered text, 10 px font, `--muted` colour, top border, `--surface2` background.

### CSS additions to the existing `<style>` block

```css
.brand-strip {
  display: flex; justify-content: space-between; align-items: center;
  padding: 4px 10px; background: #1e2232;
  border-bottom: 1px solid #2a2e42; font-size: 11px;
}
.brand-strip__name  { font-weight: 600; color: #7c85f0; }
.brand-strip__hint  { color: #7a7f9a; }

.avatar {
  width: 26px; height: 26px; border-radius: 50%;
  display: flex; align-items: center; justify-content: center;
  font-size: 11px; font-weight: 700; color: #fff; flex-shrink: 0;
}

.item { gap: 8px; }  /* existing .item gets gap */

.kb-hint {
  padding: 4px 10px; text-align: center;
  font-size: 10px; color: #7a7f9a;
  border-top: 1px solid #2a2e42; background: #1e2232;
}
```

The dropdown `min-width` increases from 220 px to 260 px to accommodate the avatar.

---

## Background service worker changes

### New cache variable
```typescript
let credentialListCache: Credential[] | null = null;
```
Cleared in `onDisconnect` and when `vault_locked` is received from the native host:
```typescript
credentialListCache = null;
credentialCache.clear();
```

### Three new cases in `chrome.runtime.onMessage`

```typescript
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
```

Note: `find_credentials` already exists in the port handler (for the autofill content script). Adding it to `onMessage` too (for the popup) reuses the same `credentialCache` map.

---

## IPC server — new endpoint

### `handle_list_credentials`

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

Add to the `handle_message` match:
```rust
"list_credentials" => handle_list_credentials(state),
```

Add after the existing vault-locked guard (the `_ if !is_unlocked` arm already blocks it).

### Unit test

```rust
#[test]
fn list_credentials_returns_credentials_list_type() {
    // Uses a mock or verifies the response type field.
    // Full integration requires a live AppState with vault unlocked.
    // At minimum, verify the handler produces the correct type key.
    let json = r#"{"type":"credentials_list","items":[]}"#;
    let v: serde_json::Value = serde_json::from_str(json).unwrap();
    assert_eq!(v["type"], "credentials_list");
}
```

(A deeper integration test is deferred — the function path is identical to `handle_find_credentials` which already has coverage.)

---

## Build changes

No changes to `package.json` or the build script. The extension:build already bundles `popup.ts` and copies `popup.css`/`popup.html`.

---

## What is NOT in scope

- Light mode
- Autofill directly from popup (remains content-script-only)
- Categories / folders
- Import / export from popup
- Desktop app UI (separate plan)
