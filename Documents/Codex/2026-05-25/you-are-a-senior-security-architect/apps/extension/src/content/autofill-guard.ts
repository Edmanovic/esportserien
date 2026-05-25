import { detectFullscreenOverlay } from './overlay-guard';

export type AutofillSignal = {
  origin: string;
  topLevelOrigin: string;
  fieldVisible: boolean;
  crossOriginIframe: boolean;
  suspiciousDomain: boolean;
};

export function isVisibleInput(element: HTMLInputElement): boolean {
  const style = window.getComputedStyle(element);
  const rect = element.getBoundingClientRect();
  return (
    style.visibility !== "hidden" &&
    style.display !== "none" &&
    Number(style.opacity) > 0 &&
    rect.width >= 8 &&
    rect.height >= 8 &&
    !element.disabled &&
    element.type !== "hidden"
  );
}

export function detectSuspiciousDomain(hostname: string): boolean {
  const ascii = hostname.toLowerCase();
  return ascii.startsWith("xn--") || ascii.includes(".xn--") || /[^\x00-\x7F]/u.test(hostname);
}

export function collectAutofillSignal(input: HTMLInputElement): AutofillSignal {
  const origin = window.location.origin;
  let topLevelOrigin = origin;
  try {
    topLevelOrigin = window.top?.location.origin ?? origin;
  } catch {
    topLevelOrigin = "cross-origin";
  }

  return {
    origin,
    topLevelOrigin,
    fieldVisible: isVisibleInput(input),
    crossOriginIframe: topLevelOrigin !== origin,
    suspiciousDomain: detectSuspiciousDomain(window.location.hostname),
  };
}

document.addEventListener(
  "click",
  (event) => {
    const target = event.target;
    if (!(target instanceof HTMLInputElement)) {
      return;
    }
    if (detectFullscreenOverlay()) {
      console.warn('[ESPASS] Potential overlay/clickjacking detected, blocking autofill');
      return;
    }
    const signal = collectAutofillSignal(target);
    if (!signal.fieldVisible || signal.crossOriginIframe || signal.suspiciousDomain) {
      return;
    }
    chrome.runtime.sendMessage({
      type: "credential-request",
      origin: signal.origin,
      topLevelOrigin: signal.topLevelOrigin,
      userGesture: event.isTrusted,
    });
  },
  { capture: true },
);

