/**
 * ESPASS content script — autofill guard.
 *
 * Listens for clicks on password fields only, requests matching credentials
 * from the background service worker, and shows the Shadow DOM dropdown.
 * Fills both username and password fields after the user selects a credential.
 *
 * Security guards (overlay detection, cross-origin iframe, suspicious domain)
 * are preserved from the original implementation.
 */

import { detectFullscreenOverlay, checkOverlay } from "./overlay-guard";
import { showDropdown, dismissDropdown, type CredentialItem } from "./dropdown";

// ---------------------------------------------------------------------------
// Signal helpers (unchanged from original)
// ---------------------------------------------------------------------------

export function isVisibleInput(element: HTMLInputElement): boolean {
  const style = window.getComputedStyle(element);
  const rect  = element.getBoundingClientRect();
  return (
    style.visibility !== "hidden" &&
    style.display    !== "none"   &&
    Number(style.opacity) > 0     &&
    rect.width  >= 8              &&
    rect.height >= 8              &&
    !element.disabled             &&
    element.type !== "hidden"
  );
}

export function detectSuspiciousDomain(hostname: string): boolean {
  const ascii = hostname.toLowerCase();
  return (
    ascii.startsWith("xn--") ||
    ascii.includes(".xn--")  ||
    /[^\x00-\x7F]/u.test(hostname)
  );
}

// ---------------------------------------------------------------------------
// Password-field detection
// ---------------------------------------------------------------------------

function isPasswordField(input: HTMLInputElement): boolean {
  if (input.type === "password") return true;
  const ac = (input.getAttribute("autocomplete") ?? "").toLowerCase();
  return ac.includes("current-password") || ac.includes("new-password");
}

// ---------------------------------------------------------------------------
// Background communication (long-lived port)
// ---------------------------------------------------------------------------

let bgPort: chrome.runtime.Port | null = null;
const pending = new Map<string, (r: Record<string, unknown>) => void>();

function getBgPort(): chrome.runtime.Port {
  if (bgPort) return bgPort;

  bgPort = chrome.runtime.connect({ name: "espass-content" });

  bgPort.onMessage.addListener((msg: Record<string, unknown>) => {
    if (msg.type === "vault_locked") {
      dismissDropdown();
      return;
    }
    const rid = msg.request_id as string | undefined;
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
    // Reject any pending requests
    for (const [id, resolve] of pending) {
      resolve({ type: "error", code: "disconnected" });
      pending.delete(id);
    }
  });

  return bgPort;
}

function sendToBg(
  msg: Record<string, unknown>
): Promise<Record<string, unknown>> {
  return new Promise((resolve) => {
    const requestId = crypto.randomUUID();
    msg.request_id = requestId;
    pending.set(requestId, resolve);
    getBgPort().postMessage(msg);
  });
}

// ---------------------------------------------------------------------------
// Click listener — entry point
// ---------------------------------------------------------------------------

document.addEventListener(
  "click",
  async (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) return;
    if (!isPasswordField(target)) return;

    // Security guards
    if (detectFullscreenOverlay()) return;
    const overlayResult = checkOverlay(event.clientX, event.clientY, target);
    if (!overlayResult.safe) return;

    const origin = window.location.origin;
    let topLevelOrigin = origin;
    try { topLevelOrigin = window.top?.location.origin ?? origin; }
    catch { topLevelOrigin = "cross-origin"; }

    if (topLevelOrigin !== origin) return; // cross-origin iframe
    if (detectSuspiciousDomain(window.location.hostname)) return;
    if (!isVisibleInput(target)) return;

    const response = await sendToBg({ type: "find_credentials", origin });
    if (response.type !== "credentials") return;

    const items = response.items as CredentialItem[];
    if (items.length === 0) return;

    showDropdown(target, items, async (id) => {
      const fillResponse = await sendToBg({ type: "fill_credential", id });
      if (fillResponse.type === "fill_data") {
        fillFields(
          target,
          fillResponse.username as string,
          fillResponse.password as string
        );
      }
    });
  },
  { capture: true }
);

// ---------------------------------------------------------------------------
// Field filling
// ---------------------------------------------------------------------------

function fillFields(
  passwordInput: HTMLInputElement,
  username: string,
  password: string
): void {
  const form = passwordInput.closest("form") ?? document.body;
  const candidates = Array.from(
    form.querySelectorAll<HTMLInputElement>(
      'input[type="text"], input[type="email"], input:not([type])'
    )
  );

  // First visible input that appears before the password field in DOM order
  const usernameInput =
    candidates.find((el) => {
      if (!isVisibleInput(el) || el === passwordInput) return false;
      return (
        el.compareDocumentPosition(passwordInput) &
        Node.DOCUMENT_POSITION_FOLLOWING
      );
    }) ?? null;

  if (usernameInput) setNativeValue(usernameInput, username);
  setNativeValue(passwordInput, password);
}

/** Set input value in a way that React / Vue / Angular detect as a real change. */
function setNativeValue(input: HTMLInputElement, value: string): void {
  const descriptor = Object.getOwnPropertyDescriptor(
    window.HTMLInputElement.prototype,
    "value"
  );
  descriptor?.set?.call(input, value);
  input.dispatchEvent(new Event("input",  { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
}
