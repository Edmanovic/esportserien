(() => {
  // apps/extension/src/content/overlay-guard.ts
  function checkOverlay(x, y, expectedContainer) {
    const top = document.elementFromPoint(x, y);
    if (!top) {
      return { safe: false, reason: "no-element-at-point" };
    }
    if (window.getComputedStyle(top).pointerEvents === "none") {
      return { safe: true };
    }
    if (top === expectedContainer || // exact match
    expectedContainer.contains(top) || // top is a child of input (future-proofing)
    top.contains(expectedContainer)) {
      return { safe: true };
    }
    return {
      safe: false,
      reason: `overlay-detected:${top.tagName.toLowerCase()}`
    };
  }
  function detectFullscreenOverlay(inputEl) {
    const viewportArea = window.innerWidth * window.innerHeight;
    if (viewportArea === 0) return false;
    const fixed = Array.from(document.querySelectorAll("*")).filter((el) => {
      const style = window.getComputedStyle(el);
      return style.position === "fixed" || style.position === "sticky";
    });
    return fixed.some((el) => {
      if (inputEl && el.contains(inputEl)) return false;
      const rect = el.getBoundingClientRect();
      const area = rect.width * rect.height;
      if (area / viewportArea <= 0.8) return false;
      const cx = rect.left + rect.width / 2;
      const cy = rect.top + rect.height / 2;
      const topEl = document.elementFromPoint(cx, cy);
      return topEl !== null && (el === topEl || el.contains(topEl));
    });
  }

  // apps/extension/src/content/dropdown.ts
  var _AVATAR_HUES = [220, 260, 170, 30, 340, 200, 290, 140];
  function _avatarColor(title) {
    let h = 0;
    for (const ch of title) h = h * 31 + ch.charCodeAt(0) & 255;
    return `hsl(${_AVATAR_HUES[h % _AVATAR_HUES.length]}, 55%, 55%)`;
  }
  function _avatarLetter(title) {
    return (title.trim()[0] ?? "?").toUpperCase();
  }
  var currentHost = null;
  var cleanupFns = [];
  function isDropdownVisible() {
    return currentHost !== null;
  }
  function dismissDropdown() {
    for (const fn of cleanupFns) fn();
    cleanupFns = [];
    currentHost?.remove();
    currentHost = null;
  }
  function showDropdown(anchor, items, onSelect) {
    dismissDropdown();
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
    brandStrip.innerHTML = '<span class="brand-strip__name">\u{1F511} ESPASS</span><span class="brand-strip__hint">ESC to close</span>';
    const itemList = document.createElement("div");
    itemList.className = "item-list";
    const kbHint = document.createElement("div");
    kbHint.className = "kb-hint";
    kbHint.textContent = "\u2191\u2193 navigate \xB7 Enter fill \xB7 Esc dismiss";
    dropdown.appendChild(brandStrip);
    dropdown.appendChild(itemList);
    dropdown.appendChild(kbHint);
    shadow.appendChild(style);
    shadow.appendChild(dropdown);
    const els = [];
    let activeIdx = -1;
    function setActive(idx) {
      els[activeIdx]?.classList.remove("active");
      activeIdx = idx;
      els[activeIdx]?.classList.add("active");
    }
    items.forEach((item, i) => {
      const el = document.createElement("div");
      el.className = "item";
      el.tabIndex = 0;
      el.setAttribute("role", "option");
      const color = _avatarColor(item.title);
      const letter = _avatarLetter(item.title);
      el.innerHTML = `<div class="avatar" data-avatar-color="${color}">${esc(letter)}</div><div class="item-text"><span class="item-title">${esc(item.title)}</span><span class="item-user">${esc(item.username)}</span></div>`;
      el.addEventListener("click", () => {
        dismissDropdown();
        onSelect(item.id);
      });
      el.addEventListener("mouseenter", () => setActive(i));
      itemList.appendChild(el);
      const avatarEl = el.querySelector(".avatar");
      if (avatarEl) avatarEl.style.background = color;
      els.push(el);
    });
    const rect = anchor.getBoundingClientRect();
    Object.assign(dropdown.style, {
      top: `${rect.bottom + 4}px`,
      left: `${rect.left}px`
    });
    const onKey = (e) => {
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
    const onClickOutside = (e) => {
      if (e.target !== anchor && !host.contains(e.target)) {
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
  function esc(s) {
    return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
  }

  // apps/extension/src/content/autofill-guard.ts
  function isVisibleInput(element) {
    const style = window.getComputedStyle(element);
    const rect = element.getBoundingClientRect();
    return style.visibility !== "hidden" && style.display !== "none" && Number(style.opacity) > 0 && rect.width >= 8 && rect.height >= 8 && !element.disabled && element.type !== "hidden";
  }
  function detectSuspiciousDomain(hostname) {
    const ascii = hostname.toLowerCase();
    return ascii.startsWith("xn--") || ascii.includes(".xn--") || /[^\x00-\x7F]/u.test(hostname);
  }
  function isPasswordField(input) {
    if (input.type === "password") return true;
    const ac = (input.getAttribute("autocomplete") ?? "").toLowerCase();
    return ac.includes("current-password") || ac.includes("new-password");
  }
  function isUsernameField(input) {
    if (input.type === "password") return false;
    const ac = (input.getAttribute("autocomplete") ?? "").toLowerCase();
    const name = (input.name ?? "").toLowerCase();
    const id = (input.id ?? "").toLowerCase();
    const strongSignal = input.type === "email" || ac.includes("username") || ac.includes("email");
    if (strongSignal) return true;
    const weakSignal = /user|email|login|account|mail/i.test(name + " " + id);
    if (!weakSignal) return false;
    const scope = input.closest("form") ?? document.body;
    return !!scope.querySelector('input[type="password"]');
  }
  function isLoginField(input) {
    return isPasswordField(input) || isUsernameField(input);
  }
  var bgPort = null;
  var pending = /* @__PURE__ */ new Map();
  function getBgPort() {
    if (bgPort) return bgPort;
    bgPort = chrome.runtime.connect({ name: "espass-content" });
    bgPort.onMessage.addListener((msg) => {
      if (msg.type === "vault_locked") {
        dismissDropdown();
        return;
      }
      const rid = msg.request_id;
      if (!rid) return;
      const resolve = pending.get(rid);
      if (resolve) {
        pending.delete(rid);
        resolve(msg);
      }
    });
    bgPort.onDisconnect.addListener(() => {
      bgPort = null;
      dismissDropdown();
      for (const [id, resolve] of pending) {
        resolve({ type: "error", code: "disconnected" });
        pending.delete(id);
      }
    });
    return bgPort;
  }
  function sendToBg(msg) {
    return new Promise((resolve) => {
      const requestId = crypto.randomUUID();
      msg.request_id = requestId;
      pending.set(requestId, resolve);
      getBgPort().postMessage(msg);
    });
  }
  async function handleLoginFieldActivation(target, clientX = 0, clientY = 0) {
    if (!isLoginField(target)) return;
    if (isDropdownVisible()) return;
    if (detectFullscreenOverlay(target)) return;
    if (clientX || clientY) {
      const overlayResult = checkOverlay(clientX, clientY, target);
      if (!overlayResult.safe) return;
    }
    const origin = window.location.origin;
    let topLevelOrigin = origin;
    try {
      topLevelOrigin = window.top?.location.origin ?? origin;
    } catch {
      topLevelOrigin = "cross-origin";
    }
    if (topLevelOrigin !== origin) return;
    if (detectSuspiciousDomain(window.location.hostname)) return;
    if (!isVisibleInput(target)) return;
    const response = await sendToBg({ type: "find_credentials", origin });
    if (response.type !== "credentials") return;
    const items = response.items;
    if (items.length === 0) return;
    showDropdown(target, items, async (id) => {
      const fillResponse = await sendToBg({ type: "fill_credential", id });
      if (fillResponse.type === "fill_data") {
        fillFields(target, fillResponse.username, fillResponse.password);
      }
    });
  }
  document.addEventListener(
    "focusin",
    async (event) => {
      const target = event.target;
      if (!(target instanceof HTMLInputElement)) return;
      await handleLoginFieldActivation(target);
    },
    { capture: true }
  );
  document.addEventListener(
    "click",
    async (event) => {
      const target = event.target;
      if (!(target instanceof HTMLInputElement)) return;
      await handleLoginFieldActivation(target, event.clientX, event.clientY);
    },
    { capture: true }
  );
  function fillFields(triggeredInput, username, password) {
    const form = triggeredInput.closest("form") ?? document.body;
    const passwordInput = triggeredInput.type === "password" ? triggeredInput : form.querySelector('input[type="password"]');
    let usernameInput = null;
    if (triggeredInput.type !== "password") {
      usernameInput = triggeredInput;
    } else if (passwordInput) {
      const candidates = Array.from(
        form.querySelectorAll(
          'input[type="text"], input[type="email"], input:not([type])'
        )
      );
      usernameInput = candidates.find((el) => {
        if (!isVisibleInput(el) || el === passwordInput) return false;
        return el.compareDocumentPosition(passwordInput) & Node.DOCUMENT_POSITION_FOLLOWING;
      }) ?? null;
    }
    if (usernameInput) setNativeValue(usernameInput, username);
    if (passwordInput) setNativeValue(passwordInput, password);
  }
  function setNativeValue(input, value) {
    const descriptor = Object.getOwnPropertyDescriptor(
      window.HTMLInputElement.prototype,
      "value"
    );
    descriptor?.set?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  }
})();
