/**
 * ESPASS autofill dropdown — Shadow DOM component.
 *
 * Isolated from page CSS and scripts. Keyboard: ↓/↑ navigate,
 * Enter selects, Escape/Tab dismiss. Click outside dismisses.
 */

export interface CredentialItem {
  id: string;
  title: string;
  username: string;
}

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

let currentHost: HTMLElement | null = null;
let cleanupFns: Array<() => void> = [];

export function isDropdownVisible(): boolean {
  return currentHost !== null;
}

export function dismissDropdown(): void {
  for (const fn of cleanupFns) fn();
  cleanupFns = [];
  currentHost?.remove();
  currentHost = null;
}

export function showDropdown(
  anchor: HTMLInputElement,
  items: CredentialItem[],
  onSelect: (id: string) => void
): void {
  dismissDropdown(); // remove any previous dropdown

  const host = document.createElement("div");
  host.setAttribute("data-espass-dropdown", "");
  const shadow = host.attachShadow({ mode: "closed" });

  const style = document.createElement("style");
  style.textContent = `
    .dropdown {
      position: fixed;
      z-index: 2147483647;
      background: #fff;
      border: 1px solid #d0d5dd;
      border-radius: 8px;
      box-shadow: 0 8px 24px rgba(0,0,0,.12);
      min-width: 260px;
      max-width: 380px;
      overflow: hidden;
      display: flex;
      flex-direction: column;
      font-family: system-ui, -apple-system, sans-serif;
      font-size: 14px;
    }
    .item {
      display: flex;
      flex-direction: row;
      padding: 8px 14px;
      cursor: pointer;
      outline: none;
      user-select: none;
      gap: 8px;
    }
    .item:hover, .item.active {
      background: #f0f4ff;
    }
    .item-title  { font-weight: 600; color: #101828; }
    .item-user   { font-size: 12px; color: #667085; margin-top: 1px; }
    .item-text   { display: flex; flex-direction: column; flex: 1; min-width: 0; }
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
  `;

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

  const els: HTMLDivElement[] = [];
  let activeIdx = -1;

  function setActive(idx: number): void {
    els[activeIdx]?.classList.remove("active");
    activeIdx = idx;
    els[activeIdx]?.classList.add("active");
  }

  items.forEach((item, i) => {
    const el = document.createElement("div");
    el.className = "item";
    el.tabIndex = 0;
    el.setAttribute("role", "option");
    const color  = _avatarColor(item.title);
    const letter = _avatarLetter(item.title);
    el.innerHTML =
      `<div class="avatar" data-avatar-color="${color}">${esc(letter)}</div>` +
      `<div class="item-text">` +
        `<span class="item-title">${esc(item.title)}</span>` +
        `<span class="item-user">${esc(item.username)}</span>` +
      `</div>`;
    el.addEventListener("click", () => { dismissDropdown(); onSelect(item.id); });
    el.addEventListener("mouseenter", () => setActive(i));
    itemList.appendChild(el);
    // Set background via DOM property (CSP-safe: not a style attribute)
    const avatarEl = el.querySelector<HTMLElement>('.avatar');
    if (avatarEl) avatarEl.style.background = color;
    els.push(el);
  });

  // Position below the anchor field
  const rect = anchor.getBoundingClientRect();
  Object.assign(dropdown.style, {
    top: `${rect.bottom + 4}px`,
    left: `${rect.left}px`,
  });

  // Keyboard handler on the document
  const onKey = (e: KeyboardEvent): void => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive(Math.min(activeIdx + 1, els.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive(Math.max(activeIdx - 1, 0));
    } else if (e.key === "Enter" && activeIdx >= 0) {
      e.preventDefault();
      dismissDropdown();
      onSelect(items[activeIdx].id);
    } else if (e.key === "Escape" || e.key === "Tab") {
      dismissDropdown();
    }
  };

  // Click-outside handler
  const onClickOutside = (e: MouseEvent): void => {
    if (e.target !== anchor && !host.contains(e.target as Node)) {
      dismissDropdown();
    }
  };

  document.addEventListener("keydown", onKey);
  document.addEventListener("click", onClickOutside, { capture: true });
  cleanupFns.push(() => {
    document.removeEventListener("keydown", onKey);
    document.removeEventListener("click", onClickOutside, { capture: true });
  });

  document.body.appendChild(host);
  currentHost = host;
}

function esc(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
