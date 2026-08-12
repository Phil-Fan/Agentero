/** Ignore layout hover briefly after wheel / scroll activity. */
export const LAYOUT_HOVER_SCROLL_SUPPRESS_MS = 450;

export function isLayoutHoverSuppressedByScroll(
	lastScrollAt: number,
	now: number,
	windowMs: number = LAYOUT_HOVER_SCROLL_SUPPRESS_MS,
): boolean {
	return lastScrollAt > 0 && now - lastScrollAt < windowMs;
}
