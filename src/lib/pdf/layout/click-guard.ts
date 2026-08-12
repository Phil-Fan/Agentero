/** Maximum pointer movement allowed for a layout-region click. */
export const LAYOUT_REGION_CLICK_MOVE_TOLERANCE_PX = 6;

type PendingActivation = {
	pointerId: number;
	startX: number;
	startY: number;
	valid: boolean;
};

/**
 * Tracks whether a pointer click stayed a click instead of becoming a scroll,
 * drag, or wheel gesture while the pointer was held down.
 */
export function createLayoutRegionClickGuard() {
	let pending: PendingActivation | null = null;
	let lastPointerActivation: { valid: boolean } | null = null;

	return {
		begin(pointerId: number, x: number, y: number) {
			pending = { pointerId, startX: x, startY: y, valid: true };
			lastPointerActivation = null;
		},

		move(pointerId: number, x: number, y: number) {
			if (!pending || pending.pointerId !== pointerId || !pending.valid) {
				return;
			}
			const dx = x - pending.startX;
			const dy = y - pending.startY;
			if (Math.hypot(dx, dy) > LAYOUT_REGION_CLICK_MOVE_TOLERANCE_PX) {
				pending.valid = false;
			}
		},

		invalidate() {
			if (pending) pending.valid = false;
		},

		end(pointerId: number) {
			if (!pending || pending.pointerId !== pointerId) return;
			lastPointerActivation = { valid: pending.valid };
			pending = null;
		},

		consume() {
			const activation = lastPointerActivation;
			lastPointerActivation = null;
			return activation?.valid === true;
		},
	};
}
