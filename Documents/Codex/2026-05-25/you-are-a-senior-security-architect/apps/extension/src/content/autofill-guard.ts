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
import { showDropdown, dismissDropdown, isDropdownVisible, type CredentialItem } from "./dropdown";

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
// Login-field detection (password OR username/email adjacent to a password field)
// ---------------------------------------------------------------------------

function isPasswordField(input: HTMLInputElement): boolean {
  if (input.type === "password") return true;
  const ac = (input.getAttribute("autocomplete") ?? "").toLowerCase();
  return ac.includes("current-password") || ac.includes("new-password");
}

function isUsernameField(input: HTMLInputElement): boolean {
  if (input.type === "password") return false;
  const ac   = (input.getAttribute("autocomplete") ?? "").toLowerCase();
  const name = (input.name  ?? "").toLowerCase();
  const id   = (input.id    ?? "").toLowerCase();

  const looksLikeUsername =
    input.type === "email" ||
    ac.includes("username") ||
    ac.includes("email") ||
    /user|email|login|account|mail/i.test(name + " " + id);

  if (!looksLikeUsername) return false;

  // Only show for username fields when a password field exists in the same form/page
  const scope = input.closest("form") ?? document.body;
  return !!scope.querySelector('input[type="password"]');
}

function isLoginField(input: HTMLInputElement): boolean {
  return isPasswordField(input) || isUsernameField(input);
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
// Trigger handler — fires on focusin (covers click, tab, programmatic focus)
// ---------------------------------------------------------------------------

async function handleLoginFieldActivation(target: HTMLInputElement, clientX = 0, clientY = 0): Promise<void> {
  if (!isLoginField(target)) return;
  if (isDropdownVisible()) return; // dropdown already showing — don't replace it

  // Security guards
  if (detectFullscreenOverlay()) return;
  if (clientX || clientY) {
    const overlayResult = checkOverlay(clientX, clientY, target);
    if (!overlayResult.safe) return;
  }

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
      fillFields(target, fillResponse.username as string, fillResponse.password as string);
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

// ---------------------------------------------------------------------------
// Field filling
// ---------------------------------------------------------------------------

function fillFields(
  triggeredInput: HTMLInputElement,
  username: string,
  password: string
): void {
  const form = triggeredInput.closest("form") ?? document.body;

  // Resolve password field — either the triggered input itself or the first one in form
  const passwordInput: HTMLInputElement | null =
    triggeredInput.type === "password"
      ? triggeredInput
      : form.querySelector<HTMLInputElement>('input[type="password"]');

  // Resolve username field — either the triggered input (if it's a username field)
  // or the first visible text/email input before the password field
  let usernameInput: HTMLInputElement | null = null;
  if (triggeredInput.type !== "password") {
    usernameInput = triggeredInput;
  } else if (passwordInput) {
    const candidates = Array.from(
      form.querySelectorAll<HTMLInputElement>(
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
