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
 * Returns true if a fixed-position element covering the viewport is detected.
 *
 * Scans visible fixed-position elements and flags those that cover more than
 * 80% of the viewport (a heuristic for clickjacking overlays).
 */
export function detectFullscreenOverlay(): boolean {
  const viewportArea = window.innerWidth * window.innerHeight;
  if (viewportArea === 0) return false;

  const fixed = Array.from(document.querySelectorAll('*')).filter((el) => {
    const style = window.getComputedStyle(el);
    return style.position === 'fixed' || style.position === 'sticky';
  });

  return fixed.some((el) => {
    const rect = el.getBoundingClientRect();
    const area = rect.width * rect.height;
    return area / viewportArea > 0.8;
  });
}
