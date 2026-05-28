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
 * coordinates and validates that it is related to the expected input:
 *   - the input itself
 *   - a child of the input (e.g. shadow content — impossible for void elements, future-proofing)
 *   - an ancestor of the input (form wrapper, label container, Knockout decorator div…)
 *   - a non-interactive element (pointer-events:none) that cannot intercept user input
 *
 * A genuine overlay would be a *sibling* element — unrelated to the input's
 * ancestor chain — which would fail all of the above checks.
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
  // Non-interactive elements cannot intercept clicks or our dropdown.
  if (window.getComputedStyle(top).pointerEvents === 'none') {
    return { safe: true };
  }
  if (
    top === expectedContainer ||           // exact match
    expectedContainer.contains(top) ||    // top is a child of input (future-proofing)
    top.contains(expectedContainer)        // top is an ancestor (form, wrapper div, label…)
  ) {
    return { safe: true };
  }
  return {
    safe: false,
    reason: `overlay-detected:${top.tagName.toLowerCase()}`,
  };
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
