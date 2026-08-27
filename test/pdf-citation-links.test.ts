import type { PdfDestinationObject, PdfLinkTarget } from "@embedpdf/models";
import { PdfActionType, PdfZoomMode } from "@embedpdf/models";
import { describe, expect, it } from "vitest";
import { getLinkDestination } from "@/components/viewer/pdf/layers/citation-links";

function dest(mode: PdfZoomMode, view: number[]): PdfDestinationObject {
	return {
		pageIndex: 2,
		zoom: { mode } as PdfDestinationObject["zoom"],
		view,
	};
}

function target(destination: PdfDestinationObject): PdfLinkTarget {
	return { type: "destination", destination };
}

function actionTarget(destination: PdfDestinationObject): PdfLinkTarget {
	return {
		type: "action",
		action: { type: PdfActionType.Goto, destination },
	};
}

describe("getLinkDestination", () => {
	it("returns null for missing target", () => {
		expect(getLinkDestination(undefined)).toBeNull();
	});

	it("reads /XYZ y from direct destination", () => {
		const d = dest(PdfZoomMode.XYZ, [0, 500, 1]);
		// Type cast: the real runtime object carries params for XYZ mode.
		(
			d as PdfDestinationObject & {
				zoom: { params: { x: number; y: number; zoom: number } };
			}
		).zoom = {
			mode: PdfZoomMode.XYZ,
			params: { x: 0, y: 500, zoom: 1 },
		};
		expect(getLinkDestination(target(d))).toEqual({ pageIndex: 2, pdfY: 500 });
	});

	it("reads /XYZ y from GoTo action", () => {
		const d = dest(PdfZoomMode.XYZ, [0, 600, 0]);
		(
			d as PdfDestinationObject & {
				zoom: { params: { x: number; y: number; zoom: number } };
			}
		).zoom = {
			mode: PdfZoomMode.XYZ,
			params: { x: 0, y: 600, zoom: 0 },
		};
		expect(getLinkDestination(actionTarget(d))).toEqual({
			pageIndex: 2,
			pdfY: 600,
		});
	});

	it("reads /FitR top from view array", () => {
		const d = dest(PdfZoomMode.FitRectangle, [0, 10, 500, 800]);
		expect(getLinkDestination(target(d))).toEqual({ pageIndex: 2, pdfY: 800 });
	});

	it("reads /FitH top from view array", () => {
		const d = dest(PdfZoomMode.FitHorizontal, [750]);
		expect(getLinkDestination(target(d))).toEqual({ pageIndex: 2, pdfY: 750 });
	});

	it("falls back to pdfY 0 for page-only destinations", () => {
		const d = dest(PdfZoomMode.FitPage, []);
		expect(getLinkDestination(target(d))).toEqual({ pageIndex: 2, pdfY: 0 });
	});

	it("falls back to pdfY 0 when /FitR view is incomplete", () => {
		const d = dest(PdfZoomMode.FitRectangle, [0, 10]);
		expect(getLinkDestination(target(d))).toEqual({ pageIndex: 2, pdfY: 0 });
	});
});
