import { describe, expect, it } from "vitest";

import {
	PADDLE_LAYOUT_MIN_SCORE,
	paddleBoxesToRegions,
} from "@/lib/pdf/layout/paddle";

describe("paddleBoxesToRegions", () => {
	const base = {
		pageIndex: 0,
		pageWidth: 612,
		pageHeight: 792,
		widthPx: 1224,
		heightPx: 1584,
		idPrefix: "paddle",
	};

	it("converts pixel boxes to point rects and normalized bboxes", () => {
		const regions = paddleBoxesToRegions({
			...base,
			boxes: [
				{
					clsId: 1,
					label: "image",
					score: 0.9,
					coordinate: [122.4, 158.4, 734.4, 950.4],
				},
			],
		});
		expect(regions).toHaveLength(1);
		const region = regions[0];
		if (!region) throw new Error("expected region");
		// Rendered at 2x: pixels / 2 = points.
		expect(region.rect).toEqual({ x: 61.2, y: 79.2, w: 306, h: 396 });
		expect(region.bbox.x).toBeCloseTo(0.1, 5);
		expect(region.bbox.y).toBeCloseTo(0.1, 5);
		expect(region.bbox.w).toBeCloseTo(0.5, 5);
		expect(region.bbox.h).toBeCloseTo(0.5, 5);
		expect(region.kind).toBe("image");
		expect(region.id).toBe("paddle-0-0");
	});

	it("drops unmapped labels, low scores, and empty boxes", () => {
		const regions = paddleBoxesToRegions({
			...base,
			boxes: [
				{
					clsId: 99,
					label: "unknown_thing",
					score: 0.99,
					coordinate: [0, 0, 10, 10],
				},
				{
					clsId: 1,
					label: "image",
					score: PADDLE_LAYOUT_MIN_SCORE - 0.01,
					coordinate: [0, 0, 10, 10],
				},
				{ clsId: 2, label: "table", score: 0.9, coordinate: [5, 5, 5, 5] },
			],
		});
		expect(regions).toHaveLength(0);
	});

	it("assigns reading order top-to-bottom", () => {
		const regions = paddleBoxesToRegions({
			...base,
			boxes: [
				{ clsId: 2, label: "text", score: 0.9, coordinate: [0, 800, 100, 900] },
				{
					clsId: 1,
					label: "image",
					score: 0.95,
					coordinate: [0, 100, 100, 200],
				},
			],
		});
		expect(regions.map((r) => r.label)).toEqual(["image", "text"]);
		expect(regions.map((r) => r.readingOrder)).toEqual([0, 1]);
	});
});
