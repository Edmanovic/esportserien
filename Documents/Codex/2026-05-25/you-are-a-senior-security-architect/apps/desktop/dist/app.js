'use strict';

const invoke = (...args) =>
  window.__TAURI__?.core?.invoke(...args) ?? Promise.reject('Tauri not available');

let _closeToolsHandler = null;

// ─── State ───────────────────────────────────────────────────────────────────
const state = {
  credentials: [],
  selectedId: null,
  search: '',
  showAddModal: false,
  modalMode: 'add',   // 'add' | 'edit'
  editingId: null,    // credential id being edited
  revealPassword: false,
};

// ─── Root ─────────────────────────────────────────────────────────────────────
const app = document.getElementById('app');

// ─── Helpers ──────────────────────────────────────────────────────────────────
function esc(str) {
  return String(str ?? '')
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function $(sel, root = document) {
  return root.querySelector(sel);
}

async function copyText(text, btn) {
  try {
    await navigator.clipboard.writeText(text);
    if (btn) {
      const orig = btn.textContent;
      btn.textContent = 'Copied!';
      btn.classList.add('copied');
      setTimeout(() => {
        btn.textContent = orig;
        btn.classList.remove('copied');
      }, 2000);
    }
  } catch {
    // clipboard not available
  }
}

function downloadBlob(filename, text, mime = 'text/plain') {
  const blob = new Blob([text], { type: mime });
  const url  = URL.createObjectURL(blob);
  const a    = document.createElement('a');
  a.href = url; a.download = filename;
  document.body.appendChild(a);
  a.click();
  setTimeout(() => { URL.revokeObjectURL(url); a.remove(); }, 1000);
}

// ─── Screen: Loading ──────────────────────────────────────────────────────────
function renderLoading() {
  app.innerHTML = `
    <div class="screen screen--center" id="screen-loading">
      <div class="brand">ESPASS</div>
      <div class="spinner"></div>
    </div>`;
}

// ─── Screen: Setup ────────────────────────────────────────────────────────────
function renderSetup() {
  app.innerHTML = `
    <div class="screen screen--center" id="screen-setup">
      <div class="brand">ESPASS</div>
      <p class="subtitle">Create your master password to set up your vault</p>
      <form class="auth-form" id="setup-form" autocomplete="off" novalidate>
        <div class="field">
          <label for="setup-pw">Master Password</label>
          <input id="setup-pw" type="password" placeholder="Choose a strong password" autocomplete="new-password" autofocus required>
        </div>
        <div class="field">
          <label for="setup-pw2">Confirm Password</label>
          <input id="setup-pw2" type="password" placeholder="Confirm your password" autocomplete="new-password" required>
        </div>
        <div class="warning">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" width="16" height="16"><path fill-rule="evenodd" d="M8.485 2.495c.673-1.167 2.357-1.167 3.03 0l6.28 10.875c.673 1.167-.17 2.625-1.516 2.625H3.72c-1.347 0-2.189-1.458-1.515-2.625L8.485 2.495zM10 5a.75.75 0 01.75.75v3.5a.75.75 0 01-1.5 0v-3.5A.75.75 0 0110 5zm0 9a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd"/></svg>
          Choose a strong password — it cannot be recovered
        </div>
        <div id="setup-error" class="error-msg" hidden></div>
        <button type="submit" class="btn btn--primary btn--full" id="setup-btn">Create Vault</button>
      </form>
    </div>`;

  const form = $('#setup-form');
  const btn = $('#setup-btn');
  const errEl = $('#setup-error');

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const pw = $('#setup-pw').value;
    const pw2 = $('#setup-pw2').value;
    errEl.hidden = true;

    if (!pw) { showError(errEl, 'Password cannot be empty.'); return; }
    if (pw.length < 8) { showError(errEl, 'Password must be at least 8 characters.'); return; }
    if (pw !== pw2) { showError(errEl, 'Passwords do not match.'); return; }

    btn.disabled = true;
    btn.textContent = 'Creating vault…';
    try {
      await invoke('create_vault', { password: pw });
      await bootUnlocked();
    } catch (err) {
      showError(errEl, `Error: ${err}`);
      btn.disabled = false;
      btn.textContent = 'Create Vault';
    }
  });
}

// ─── Screen: Locked ───────────────────────────────────────────────────────────
function renderLocked() {
  app.innerHTML = `
    <div class="screen screen--center" id="screen-locked">
      <div class="brand">ESPASS</div>
      <p class="subtitle">Enter your master password to unlock</p>
      <form class="auth-form" id="unlock-form" autocomplete="off" novalidate>
        <div class="field">
          <label for="unlock-pw">Master Password</label>
          <input id="unlock-pw" type="password" placeholder="Master password" autocomplete="current-password" autofocus required>
        </div>
        <div id="unlock-error" class="error-msg" hidden></div>
        <button type="submit" class="btn btn--primary btn--full" id="unlock-btn">Unlock</button>
      </form>
    </div>`;

  const form = $('#unlock-form');
  const btn = $('#unlock-btn');
  const errEl = $('#unlock-error');

  form.addEventListener('submit', async (e) => {
    e.preventDefault();
    const pw = $('#unlock-pw').value;
    errEl.hidden = true;

    if (!pw) { showError(errEl, 'Please enter your password.'); return; }

    btn.disabled = true;
    btn.textContent = 'Unlocking…';
    try {
      await invoke('unlock_vault', { password: pw });
      await bootUnlocked();
    } catch {
      showError(errEl, 'Incorrect password.');
      btn.disabled = false;
      btn.textContent = 'Unlock';
      $('#unlock-pw').value = '';
      $('#unlock-pw').focus();
    }
  });
}

// ─── Screen: Unlocked ────────────────────────────────────────────────────────
function renderUnlocked() {
  const filtered = filterCredentials();
  const selected = state.selectedId
    ? state.credentials.find(c => c.id === state.selectedId)
    : null;

  app.innerHTML = `
    <div class="screen screen--vault" id="screen-vault">
      <!-- Top bar -->
      <header class="topbar">
        <div class="brand brand--small">ESPASS</div>
        <div class="topbar__search">
          <input id="search-input" type="search" placeholder="Search credentials…" value="${esc(state.search)}">
        </div>
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
      </header>

      <!-- Main area -->
      <div class="vault-body">
        <!-- Credential list -->
        <aside class="cred-list">
          ${filtered.length === 0
            ? `<div class="empty-state">${state.search ? 'No results found.' : 'No credentials yet. Click <strong>+ Add</strong> to get started.'}</div>`
            : filtered.map(c => `
              <div class="cred-card ${c.id === state.selectedId ? 'cred-card--selected' : ''}" data-id="${esc(c.id)}" role="button" tabindex="0">
                <div class="cred-card__body">
                  <div class="cred-card__title">${esc(c.title)}</div>
                  <div class="cred-card__username muted">${esc(c.username)}</div>
                  ${c.url ? `<div class="cred-card__url muted">${esc(c.url)}</div>` : ''}
                </div>
                <button class="btn btn--icon copy-user-btn" data-username="${esc(c.username)}" title="Copy username" tabindex="-1">
                  <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" width="14" height="14"><path d="M7 3.5A1.5 1.5 0 018.5 2h3.879a1.5 1.5 0 011.06.44l3.122 3.12A1.5 1.5 0 0117 6.622V12.5a1.5 1.5 0 01-1.5 1.5h-1v-3.379a3 3 0 00-.879-2.121L10.5 5.379A3 3 0 008.379 4.5H7v-1z"/><path d="M4.5 6A1.5 1.5 0 003 7.5v9A1.5 1.5 0 004.5 18h7a1.5 1.5 0 001.5-1.5v-5.879a1.5 1.5 0 00-.44-1.06L9.44 6.439A1.5 1.5 0 008.378 6H4.5z"/></svg>
                </button>
              </div>`).join('')
          }
        </aside>

        <!-- Detail panel -->
        <main class="detail-panel">
          ${selected ? renderDetailHTML(selected) : `<div class="empty-state empty-state--detail">Select a credential to view details</div>`}
        </main>
      </div>

      <!-- Add modal -->
      ${state.showAddModal ? renderAddModalHTML() : ''}
      <!-- Hidden import file input -->
      <input type="file" id="import-file-input" accept=".csv" style="display:none">
    </div>`;

  bindVaultEvents();
}

function renderDetailHTML(cred) {
  return `
    <div class="detail" id="detail-view" data-id="${esc(cred.id)}">
      <h2 class="detail__title">${esc(cred.title)}</h2>
      <div class="detail__fields">
        <div class="detail__field">
          <span class="detail__label">Username</span>
          <div class="detail__value-row">
            <span class="detail__value">${esc(cred.username)}</span>
            <button class="btn btn--sm copy-btn" data-copy="${esc(cred.username)}">Copy</button>
          </div>
        </div>
        <div class="detail__field">
          <span class="detail__label">Password</span>
          <div class="detail__value-row">
            <span class="detail__value detail__password ${state.revealPassword ? '' : 'detail__password--hidden'}" id="pw-display">${state.revealPassword ? esc(cred.password ?? '') : '••••••••••••'}</span>
            <button class="btn btn--sm" id="reveal-btn">${state.revealPassword ? 'Hide' : 'Show'}</button>
            <button class="btn btn--sm copy-btn" data-copy-pw="1" data-id="${esc(cred.id)}">Copy</button>
          </div>
        </div>
        ${cred.url ? `
        <div class="detail__field">
          <span class="detail__label">URL</span>
          <div class="detail__value-row">
            <a class="detail__value detail__link" href="${esc(cred.url)}" target="_blank" rel="noopener noreferrer">${esc(cred.url)}</a>
          </div>
        </div>` : ''}
      </div>
      <div class="detail__footer">
        <button class="btn btn--ghost" id="edit-btn" data-id="${esc(cred.id)}">Edit</button>
        <button class="btn btn--danger" id="delete-btn" data-id="${esc(cred.id)}">Delete</button>
      </div>
    </div>`;
}

function renderAddModalHTML() {
  const isEdit = state.modalMode === 'edit';
  const editing = isEdit ? state.credentials.find(c => c.id === state.editingId) : null;
  if (isEdit && !editing) {
    // Credential no longer in state — abort edit silently.
    state.modalMode = 'add';
    state.editingId = null;
    state.showAddModal = false;
    return '';
  }
  const prefillTitle = editing?.title ?? '';
  const prefillUsername = editing?.username ?? '';
  const prefillUrl = editing?.url ?? '';

  return `
    <div class="modal-overlay" id="modal-overlay">
      <div class="modal" role="dialog" aria-modal="true" aria-label="${isEdit ? 'Edit credential' : 'Add credential'}">
        <h3 class="modal__title">${isEdit ? 'Edit Credential' : 'Add Credential'}</h3>
        <form id="add-form" autocomplete="off" novalidate>
          <div class="field">
            <label for="add-title">Title <span class="required">*</span></label>
            <input id="add-title" type="text" placeholder="e.g. GitHub" required autofocus value="${esc(prefillTitle)}">
          </div>
          <div class="field">
            <label for="add-username">Username</label>
            <input id="add-username" type="text" placeholder="e.g. user@example.com" autocomplete="off" value="${esc(prefillUsername)}">
          </div>
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
          <div class="field">
            <label for="add-url">URL <span class="muted">(optional)</span></label>
            <input id="add-url" type="url" placeholder="https://example.com" autocomplete="off" value="${esc(prefillUrl)}">
          </div>
          <div id="add-error" class="error-msg" hidden></div>
          <div class="modal__actions">
            <button type="button" class="btn btn--ghost" id="add-cancel">Cancel</button>
            <button type="submit" class="btn btn--primary" id="add-save">${isEdit ? 'Save changes' : 'Save'}</button>
          </div>
        </form>
      </div>
    </div>`;
}

function filterCredentials() {
  const q = state.search.toLowerCase().trim();
  if (!q) return state.credentials;
  return state.credentials.filter(c =>
    c.title.toLowerCase().includes(q) ||
    c.username.toLowerCase().includes(q) ||
    (c.url ?? '').toLowerCase().includes(q)
  );
}

function bindVaultEvents() {
  // Search
  const searchInput = $('#search-input');
  if (searchInput) {
    searchInput.addEventListener('input', (e) => {
      state.search = e.target.value;
      state.selectedId = null;
      renderUnlocked();
    });
  }

  // Tools dropdown toggle
  $('#tools-btn')?.addEventListener('click', (e) => {
    e.stopPropagation();
    const menu = $('#tools-menu');
    if (!menu) return;
    menu.style.display = menu.style.display === 'none' ? '' : 'none';
  });
  if (_closeToolsHandler) document.removeEventListener('click', _closeToolsHandler);
  _closeToolsHandler = function() {
    const menu = $('#tools-menu');
    if (menu) menu.style.display = 'none';
  };
  document.addEventListener('click', _closeToolsHandler);

  // Import CSV
  $('#import-csv-btn')?.addEventListener('click', () => {
    const menu = $('#tools-menu');
    if (menu) menu.style.display = 'none';
    $('#import-file-input')?.click();
  });

  $('#import-file-input')?.addEventListener('change', async (e) => {
    const file = e.target.files?.[0];
    if (!file) return;
    const text = await file.text();
    e.target.value = '';
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
    const menu = $('#tools-menu');
    if (menu) menu.style.display = 'none';
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
    const menu = $('#tools-menu');
    if (menu) menu.style.display = 'none';
    if (!confirm('This file will contain your passwords in plaintext.\n\nStore it in a secure location.')) return;
    try {
      const json = await invoke('export_credentials_json');
      downloadBlob('espass-export.json', json, 'application/json');
    } catch (err) {
      showToast(`Export failed: ${err}`, 'error');
    }
  });

  // Lock
  $('#lock-btn')?.addEventListener('click', async () => {
    await invoke('lock_vault');
    state.credentials = [];
    state.selectedId = null;
    state.search = '';
    renderLocked();
  });

  // Add button
  $('#add-btn')?.addEventListener('click', () => {
    state.modalMode = 'add';
    state.editingId = null;
    state.showAddModal = true;
    renderUnlocked();
    $('#add-title')?.focus();
  });

  // Credential cards — select
  document.querySelectorAll('.cred-card').forEach(card => {
    const id = card.dataset.id;
    const activate = () => {
      if (state.selectedId !== id) {
        state.selectedId = id;
        state.revealPassword = false;
      } else {
        state.selectedId = null;
      }
      renderUnlocked();
    };
    card.addEventListener('click', (e) => {
      if (e.target.closest('.copy-user-btn')) return;
      activate();
    });
    card.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); activate(); }
    });
  });

  // Copy username from card
  document.querySelectorAll('.copy-user-btn').forEach(btn => {
    btn.addEventListener('click', (e) => {
      e.stopPropagation();
      copyText(btn.dataset.username, btn);
    });
  });

  // Detail panel: copy buttons
  document.querySelectorAll('.copy-btn').forEach(btn => {
    btn.addEventListener('click', async () => {
      if (btn.dataset.copyPw) {
        // Need to fetch full credential to get password
        try {
          const cred = await invoke('get_credential', { id: btn.dataset.id });
          copyText(cred.password, btn);
        } catch { /* ignore */ }
      } else {
        copyText(btn.dataset.copy, btn);
      }
    });
  });

  // Detail panel: reveal password
  $('#reveal-btn')?.addEventListener('click', async () => {
    if (!state.revealPassword) {
      // Fetch full credential first
      try {
        const cred = await invoke('get_credential', { id: state.selectedId });
        state.revealPassword = true;
        // Update display inline
        const display = $('#pw-display');
        if (display) display.textContent = cred.password ?? '';
        display?.classList.remove('detail__password--hidden');
        const revBtn = $('#reveal-btn');
        if (revBtn) revBtn.textContent = 'Hide';
        // Store on current cred for copy
        const found = state.credentials.find(c => c.id === state.selectedId);
        if (found) found._password = cred.password;
      } catch { /* ignore */ }
    } else {
      state.revealPassword = false;
      const display = $('#pw-display');
      if (display) { display.textContent = '••••••••••••'; display.classList.add('detail__password--hidden'); }
      const revBtn = $('#reveal-btn');
      if (revBtn) revBtn.textContent = 'Show';
    }
  });

  // Detail panel: edit
  $('#edit-btn')?.addEventListener('click', async (e) => {
    const id = e.currentTarget.dataset.id;
    try {
      const cred = await invoke('get_credential', { id });
      state.modalMode = 'edit';
      state.editingId = id;
      state.showAddModal = true;
      renderUnlocked();
      const pwInput = $('#add-password');
      if (pwInput) pwInput.value = cred.password ?? '';
      $('#add-title')?.focus();
    } catch (err) {
      alert(`Could not load credential: ${err}`);
    }
  });

  // Detail panel: delete
  $('#delete-btn')?.addEventListener('click', async (e) => {
    const id = e.currentTarget.dataset.id;
    const cred = state.credentials.find(c => c.id === id);
    if (!confirm(`Delete "${cred?.title ?? 'this credential'}"? This cannot be undone.`)) return;
    try {
      await invoke('delete_credential', { id });
      state.credentials = state.credentials.filter(c => c.id !== id);
      state.selectedId = null;
      renderUnlocked();
    } catch (err) {
      alert(`Error deleting: ${err}`);
    }
  });

  // Add modal
  if (state.showAddModal) {
    // Close on overlay click
    $('#modal-overlay')?.addEventListener('click', (e) => {
      if (e.target.id === 'modal-overlay') closeAddModal();
    });

    // Cancel
    $('#add-cancel')?.addEventListener('click', closeAddModal);

    // Escape key
    document.addEventListener('keydown', handleModalEscape);

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

    // Generator local state
    const gen = { length: 20, upper: true, lower: true, digits: true, symbols: true };

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
          const box = $(`#${id}`);
          if (box) box.checked = true;
          const key = id.replace('gen-', '');
          gen[key] = true;
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
          pwInput.type = 'text';
          const toggle = $('#add-pw-toggle');
          if (toggle) toggle.textContent = 'Hide';
        }
      } catch (err) {
        alert(`Generator error: ${err}`);
      } finally {
        if (btn) { btn.disabled = false; btn.textContent = 'Generate password'; }
      }
    });

    // Submit add form
    $('#add-form')?.addEventListener('submit', async (e) => {
      e.preventDefault();
      const title = $('#add-title').value.trim();
      const username = $('#add-username').value.trim();
      const password = $('#add-password').value;
      const url = $('#add-url').value.trim() || null;
      const errEl = $('#add-error');
      errEl.hidden = true;

      if (!title) { showError(errEl, 'Title is required.'); return; }

      const saveBtn = $('#add-save');
      saveBtn.disabled = true;
      saveBtn.textContent = 'Saving…';

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
    });
  }
}

function handleModalEscape(e) {
  if (e.key === 'Escape') closeAddModal();
}

function closeAddModal() {
  document.removeEventListener('keydown', handleModalEscape);
  state.showAddModal = false;
  state.modalMode = 'add';
  state.editingId = null;
  renderUnlocked();
}

// ─── Boot sequence ────────────────────────────────────────────────────────────
async function bootUnlocked() {
  try {
    state.credentials = await invoke('list_credentials');
  } catch {
    state.credentials = [];
  }
  state.selectedId = null;
  state.search = '';
  state.revealPassword = false;
  renderUnlocked();
}

function showError(el, msg) {
  el.textContent = msg;
  el.hidden = false;
}

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

async function boot() {
  renderLoading();
  try {
    const exists = await invoke('vault_exists');
    if (!exists) {
      renderSetup();
    } else {
      const status = await invoke('get_session_status');
      if (status?.unlocked) {
        await bootUnlocked();
      } else {
        renderLocked();
      }
    }
  } catch (err) {
    // Tauri not available — show a dev/preview notice
    app.innerHTML = `
      <div class="screen screen--center">
        <div class="brand">ESPASS</div>
        <p class="subtitle muted">Tauri runtime not detected.</p>
        <p class="muted" style="font-size:13px;max-width:320px;text-align:center;">
          Run this app inside a Tauri window.<br>
          <code style="color:#7c85f0">${err}</code>
        </p>
      </div>`;
  }
}

document.addEventListener('DOMContentLoaded', boot);
