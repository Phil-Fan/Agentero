import { describe, expect, it } from "vitest";

import {
	createLayoutRegionClickGuard,
	LAYOUT_REGION_CLICK_MOVE_TOLERANCE_PX,
} from "@/lib/pdf/layout";

describe("PDF layout region click guard", () => {
	it("accepts a stationary primary pointer click", () => {
		const guard = createLayoutRegionClickGuard();
		guard.begin(1, 100, 200);
		guard.move(1, 100 + LAYOUT_REGION_CLICK_MOVE_TOLERANCE_PX, 200);
		guard.end(1);

		expect(guard.consume()).toBe(true);
	});

	it("rejects pointer movement beyond click tolerance", () => {
		const guard = createLayoutRegionClickGuard();
		guard.begin(1, 100, 200);
		guard.move(1, 107, 200);
		guard.end(1);

		expect(guard.consume()).toBe(false);
	});

	it("rejects wheel and scroll gestures during a held pointer", () => {
		const guard = createLayoutRegionClickGuard();
		guard.begin(1, 100, 200);
		guard.invalidate();
		guard.end(1);

		expect(guard.consume()).toBe(false);
	});

	it("does not consume an activation for a different pointer", () => {
		const guard = createLayoutRegionClickGuard();
		guard.begin(1, 100, 200);
		guard.end(2);

		expect(guard.consume()).toBe(false);
	});

	it("allows a later activation after a rejected gesture", () => {
		const guard = createLayoutRegionClickGuard();
		guard.begin(1, 100, 200);
		guard.invalidate();
		guard.end(1);
		expect(guard.consume()).toBe(false);

		guard.begin(1, 100, 200);
		guard.end(1);
		expect(guard.consume()).toBe(true);
	});
});
