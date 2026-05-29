// apps/extension/src/popup/popup.ts
var AVATAR_HUES = [220, 260, 170, 30, 340, 200, 290, 140];
function avatarColor(title) {
  let h = 0;
  for (const ch of title) h = h * 31 + ch.charCodeAt(0) & 255;
  return `hsl(${AVATAR_HUES[h % AVATAR_HUES.length]}, 55%, 55%)`;
}
function avatarLetter(title) {
  return (title.trim()[0] ?? "?").toUpperCase();
}
function esc(s) {
  return String(s ?? "").replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;").replace(/'/g, "&#x27;");
}
function showToast(msg, type = "success") {
  document.querySelector(".toast")?.remove();
  const t = document.createElement("div");
  t.className = `toast toast--${type}`;
  t.textContent = msg;
  document.body.appendChild(t);
  setTimeout(() => t.remove(), 1500);
}
async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    const ta = document.createElement("textarea");
    ta.value = text;
    Object.assign(ta.style, { position: "fixed", opacity: "0", top: "0", left: "0" });
    document.body.appendChild(ta);
    ta.focus();
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    if (!ok) throw new Error("execCommand copy failed");
  }
}
function credItemHTML(cred, suggested = false) {
  const color = avatarColor(cred.title);
  const letter = avatarLetter(cred.title);
  const url = cred.url ?? "";
  let domain = "";
  if (url) {
    try {
      domain = new URL(url).hostname;
    } catch {
      domain = "";
    }
  }
  return `<div class="cred-item${suggested ? " cred-item--suggested" : ""}"
               data-id="${esc(cred.id)}" tabindex="0">
    <div class="cred-avatar" data-avatar-color="${esc(color)}">${esc(letter)}</div>
    <div class="cred-body">
      <div class="cred-title">${esc(cred.title)}</div>
      <div class="cred-username">${esc(cred.username)}${domain ? `<span class="cred-domain"> \xB7 ${esc(domain)}</span>` : ""}</div>
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
function attachCopyActions(container) {
  container.querySelectorAll(".copy-btn").forEach((btn) => {
    const fresh = btn.cloneNode(true);
    btn.replaceWith(fresh);
    fresh.addEventListener("click", async (e) => {
      e.stopPropagation();
      const action = fresh.dataset.action;
      const id = fresh.dataset.id;
      if (action === "copy-user") {
        try {
          await copyText(fresh.dataset.value);
          showToast("Username copied");
        } catch {
          showToast("Failed to copy", "error");
        }
      } else {
        try {
          const resp = await chrome.runtime.sendMessage({ type: "get_credential", id });
          if (resp?.type === "credential" && resp.password) {
            await copyText(resp.password);
            showToast("Password copied");
          } else {
            showToast("Failed to copy", "error");
          }
        } catch {
          showToast("Failed to copy", "error");
        }
      }
    });
  });
}
function applyAvatarColors(container) {
  container.querySelectorAll("[data-avatar-color]").forEach((el) => {
    el.style.background = el.dataset.avatarColor ?? "";
  });
}
function renderUnavailable(root) {
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
  document.getElementById("retry-btn").addEventListener("click", () => main());
}
function renderLocked(root) {
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
  const form = document.getElementById("unlock-form");
  const pwIn = document.getElementById("master-pw");
  const errEl = document.getElementById("unlock-error");
  const btn = document.getElementById("unlock-btn");
  form.addEventListener("submit", async (e) => {
    e.preventDefault();
    const password = pwIn.value;
    if (!password) return;
    btn.disabled = true;
    btn.textContent = "Unlocking\u2026";
    pwIn.value = "";
    try {
      const resp = await chrome.runtime.sendMessage({ type: "unlock", password });
      if (resp?.type === "unlock_result" && resp?.ok === true) {
        await main();
      } else {
        errEl.textContent = "Wrong password";
        errEl.hidden = false;
        btn.disabled = false;
        btn.textContent = "Unlock";
        pwIn.focus();
      }
    } catch {
      errEl.textContent = "Could not connect to ESPASS";
      errEl.hidden = false;
      btn.disabled = false;
      btn.textContent = "Unlock";
    }
  });
}
async function renderUnlocked(root, tabOrigin) {
  let tabResp;
  let allResp;
  try {
    [tabResp, allResp] = await Promise.all([
      tabOrigin ? chrome.runtime.sendMessage({ type: "find_credentials", origin: tabOrigin }) : Promise.resolve({ type: "credentials", items: [] }),
      chrome.runtime.sendMessage({ type: "list_credentials" })
    ]);
  } catch {
    root.innerHTML = `
      <div class="screen screen--center">
        <div class="screen-heading">Could not load credentials</div>
        <div class="screen-sub">The ESPASS app may have restarted.</div>
        <button class="btn btn--primary" id="retry-btn">Retry</button>
      </div>`;
    document.getElementById("retry-btn").addEventListener("click", () => main());
    return;
  }
  const tabMatches = tabResp?.type === "credentials" ? tabResp.items : [];
  const allCreds = allResp?.type === "credentials_list" ? allResp.items : [];
  const tabSection = tabMatches.length > 0 ? `<div class="section-group">
         <div class="section-label">Suggested for this page</div>
         ${tabMatches.slice(0, 3).map((c) => credItemHTML(c, true)).join("")}
       </div>` : "";
  const listHTML = allCreds.length === 0 ? '<div class="empty-state">No credentials saved yet.</div>' : allCreds.map((c) => credItemHTML(c)).join("");
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
                 placeholder="Search\u2026" autocomplete="off" />
        </div>
        <div class="cred-list" id="cred-list">${listHTML}</div>
      </div>
    </div>`;
  document.getElementById("lock-btn").addEventListener("click", async () => {
    try {
      await chrome.runtime.sendMessage({ type: "lock" });
    } catch {
    }
    await main();
  });
  attachCopyActions(root);
  applyAvatarColors(root);
  const searchEl = document.getElementById("cred-search");
  const credList = document.getElementById("cred-list");
  searchEl.addEventListener("input", () => {
    const q = searchEl.value.toLowerCase().trim();
    if (!q) {
      credList.innerHTML = allCreds.length === 0 ? '<div class="empty-state">No credentials saved yet.</div>' : allCreds.map((c) => credItemHTML(c)).join("");
    } else {
      const filtered = allCreds.filter(
        (c) => c.title.toLowerCase().includes(q) || c.username.toLowerCase().includes(q) || (c.url ?? "").toLowerCase().includes(q)
      );
      credList.innerHTML = filtered.length === 0 ? `<div class="empty-state">No matches for "${esc(q)}"</div>` : filtered.map((c) => credItemHTML(c)).join("");
    }
    attachCopyActions(credList);
    applyAvatarColors(credList);
  });
}
async function main() {
  const root = document.getElementById("root");
  root.innerHTML = '<div class="screen screen--center"><div class="spinner"></div></div>';
  let tabOrigin = null;
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const url = tab?.url ?? "";
    if (url.startsWith("https://")) {
      tabOrigin = new URL(url).origin;
    }
  } catch {
  }
  try {
    const resp = await chrome.runtime.sendMessage({ type: "get_vault_status" });
    switch (resp?.state) {
      case "ready":
        await renderUnlocked(root, tabOrigin);
        break;
      case "locked":
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
