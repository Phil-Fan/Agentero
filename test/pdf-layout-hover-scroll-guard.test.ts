import { describe, expect, it } from "vitest";

import {
	isLayoutHoverSuppressedByScroll,
	LAYOUT_HOVER_SCROLL_SUPPRESS_MS,
} from "@/lib/pdf/layout";

describe("PDF layout hover scroll guard", () => {
	it("does not suppress hover before any scroll activity", () => {
		expect(isLayoutHoverSuppressedByScroll(0, 1000)).toBe(false);
	});

	it("suppresses hover during the post-scroll guard window", () => {
		expect(isLayoutHoverSuppressedByScroll(1000, 1200)).toBe(true);
	});

	it("allows hover after the post-scroll guard window", () => {
		expect(
			isLayoutHoverSuppressedByScroll(
				1000,
				1000 + LAYOUT_HOVER_SCROLL_SUPPRESS_MS,
			),
		).toBe(false);
	});
});
