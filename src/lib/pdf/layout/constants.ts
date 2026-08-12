/**
 * Shared thresholds for the figures rail and PDF layout hover interactions.
 */

/** Default confidence gate (0–1) for sidebar gallery + hover hit targets. */
export const LAYOUT_SIDEBAR_MIN_SCORE = 0.3;

/**
 * Minimum on-screen region size (px) before the「单击进行批注」hint chip is drawn.
 * The chip is a fixed-size label living inside a box that scales with zoom, so
 * on anything smaller it would spill past the region and cover its neighbours.
 */
export const LAYOUT_HINT_MIN_REGION_W_PX = 120;
export const LAYOUT_HINT_MIN_REGION_H_PX = 28;

/**
 * Formula + Annotation.md legend: free to open (no crop), so dwell is short —
 * feels like a tooltip, not a modal draft.
 */
export const LAYOUT_FORMULA_HOVER_DWELL_MS = 280;

/**
 * Leave formula region / legend card → close after this grace window.
 * Long enough to cross the small gap into the card (citation preview uses
 * ~250ms).
 */
export const LAYOUT_FORMULA_HOVER_HIDE_MS = 320;
