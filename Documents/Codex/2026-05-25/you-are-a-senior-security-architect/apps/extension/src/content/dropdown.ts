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
      min-width: 220px;
      max-width: 380px;
      overflow: hidden;
      font-family: system-ui, -apple-system, sans-serif;
      font-size: 14px;
    }
    .item {
      display: flex;
      flex-direction: column;
      padding: 8px 14px;
      cursor: pointer;
      outline: none;
      user-select: none;
    }
    .item:hover, .item.active {
      background: #f0f4ff;
    }
    .item-title  { font-weight: 600; color: #101828; }
    .item-user   { font-size: 12px; color: #667085; margin-top: 1px; }
  `;
  shadow.appendChild(style);

  const dropdown = document.createElement("div");
  dropdown.className = "dropdown";
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
    el.innerHTML =
      `<span class="item-title">🔑 ${esc(item.title)}</span>` +
      `<span class="item-user">${esc(item.username)}</span>`;
    el.addEventListener("click", () => { dismissDropdown(); onSelect(item.id); });
    el.addEventListener("mouseenter", () => setActive(i));
    dropdown.appendChild(el);
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
