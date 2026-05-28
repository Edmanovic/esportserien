/**
 * Anti-overlay and clickjacking detection for ESPASS autofill UI.
 *
 * Detects when a transparent or opaque element is placed over the ESPASS
 * extension popup, which could intercept user input. Call `checkOverlay()`
 * before displaying any credential in the autofill UI.
 */

/** Result of an overlay check. */
export interface OverlayCheckResult {
  safe: boolean;
  reason?: string;
}

/**
 * Checks whether the element at the given viewport coordinates is overlaid
 * by a suspicious element.
 *
 * Uses `document.elementFromPoint` to find the topmost element at the
 * coordinates and validates that it belongs to the expected container.
 */
export function checkOverlay(
  x: number,
  y: number,
  expectedContainer: Element
): OverlayCheckResult {
  const top = document.elementFromPoint(x, y);
  if (!top) {
    return { safe: false, reason: 'no-element-at-point' };
  }
  if (!expectedContainer.contains(top) && top !== expectedContainer) {
    return {
      safe: false,
      reason: `overlay-detected:${top.tagName.toLowerCase()}`,
    };
  }
  return { safe: true };
}

/**
 * Returns true if a fixed-position element covering the viewport is detected
 * and is not part of a legitimate modal login form.
 *
 * Scans visible fixed-position elements and flags those that:
 *   1. Cover more than 80% of the viewport, AND
 *   2. Do NOT contain the target input (i.e. are not a modal wrapping the form), AND
 *   3. Are actually the topmost layer at their own center point (i.e. are not
 *      a backdrop sitting behind a dialog — those are harmless).
 *
 * Passing `inputEl` prevents false positives on sites that show login forms
 * inside fixed-position modal dialogs with full-viewport backdrops.
 */
export function detectFullscreenOverlay(inputEl?: Element): boolean {
  const viewportArea = window.innerWidth * window.innerHeight;
  if (viewportArea === 0) return false;

  const fixed = Array.from(document.querySelectorAll('*')).filter((el) => {
    const style = window.getComputedStyle(el);
    return style.position === 'fixed' || style.position === 'sticky';
  });

  return fixed.some((el) => {
    // A large fixed element that wraps the input is a legitimate modal container.
    if (inputEl && el.contains(inputEl)) return false;

    const rect = el.getBoundingClientRect();
    const area = rect.width * rect.height;
    if (area / viewportArea <= 0.8) return false;

    // Only flag the element if it is actually the topmost layer at its own centre.
    // Modal backdrops (z-index behind the dialog) fail this check because
    // elementFromPoint returns the dialog on top of them — not the backdrop itself.
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const topEl = document.elementFromPoint(cx, cy);
    return topEl !== null && (el === topEl || el.contains(topEl));
  });
}
