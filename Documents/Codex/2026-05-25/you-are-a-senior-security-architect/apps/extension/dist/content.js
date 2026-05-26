(() => {
  // apps/extension/src/content/overlay-guard.ts
  function checkOverlay(x, y, expectedContainer) {
    const top = document.elementFromPoint(x, y);
    if (!top) {
      return { safe: false, reason: "no-element-at-point" };
    }
    if (!expectedContainer.contains(top) && top !== expectedContainer) {
      return {
        safe: false,
        reason: `overlay-detected:${top.tagName.toLowerCase()}`
      };
    }
    return { safe: true };
  }
  function detectFullscreenOverlay() {
    const viewportArea = window.innerWidth * window.innerHeight;
    if (viewportArea === 0) return false;
    const fixed = Array.from(document.querySelectorAll("*")).filter((el) => {
      const style = window.getComputedStyle(el);
      return style.position === "fixed" || style.position === "sticky";
    });
    return fixed.some((el) => {
      const rect = el.getBoundingClientRect();
      const area = rect.width * rect.height;
      return area / viewportArea > 0.8;
    });
  }

  // apps/extension/src/content/dropdown.ts
  var currentHost = null;
  var cleanupFns = [];
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
      el.innerHTML = `<span class="item-title">\u{1F511} ${esc(item.title)}</span><span class="item-user">${esc(item.username)}</span>`;
      el.addEventListener("click", () => {
        dismissDropdown();
        onSelect(item.id);
      });
      el.addEventListener("mouseenter", () => setActive(i));
      dropdown.appendChild(el);
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
  document.addEventListener(
    "click",
    async (event) => {
      const target = event.target;
      if (!(target instanceof HTMLInputElement)) return;
      if (!isPasswordField(target)) return;
      if (detectFullscreenOverlay()) return;
      const overlayResult = checkOverlay(event.clientX, event.clientY, target);
      if (!overlayResult.safe) return;
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
          fillFields(
            target,
            fillResponse.username,
            fillResponse.password
          );
        }
      });
    },
    { capture: true }
  );
  function fillFields(passwordInput, username, password) {
    const form = passwordInput.closest("form") ?? document.body;
    const candidates = Array.from(
      form.querySelectorAll(
        'input[type="text"], input[type="email"], input:not([type])'
      )
    );
    const usernameInput = candidates.find((el) => {
      if (!isVisibleInput(el) || el === passwordInput) return false;
      return el.compareDocumentPosition(passwordInput) & Node.DOCUMENT_POSITION_FOLLOWING;
    }) ?? null;
    if (usernameInput) setNativeValue(usernameInput, username);
    setNativeValue(passwordInput, password);
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
