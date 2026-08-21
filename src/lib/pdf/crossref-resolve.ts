/**
 * Resolve a hyperref cross-reference destination to the layout region it points
 * at, so a hovered `\ref` link can preview the figure / table / equation /
 * algorithm crop.
 *
 * The destination carries a page + PDF-native y (origin bottom-left). Layout
 * regions are normalized top-left, so the y is converted and used to pick the
 * region of the matching kind whose vertical span contains the anchor — the
 * hyperref `\label` sits inside the merged float bbox (caption included). When
 * nothing contains it, the nearest region of that kind by vertical centre wins;
 * with no page height we fall back to the largest candidate.
 */

import { clamp01 } from "@/lib/core/math";
import type { CrossrefKind } from "@/lib/pdf/citation-dest-keys";
import {
	bboxArea,
	hoverableLayoutRegionsOnPage,
} from "@/lib/pdf/layout/hit-test";
import {
	isAlgorithmLayoutKind,
	isFigureLayoutKind,
	isFormulaLayoutKind,
	isTableLayoutKind,
} from "@/lib/pdf/layout/labels";
import type { PdfLayoutKind, PdfLayoutRegion } from "@/lib/pdf/layout/types";

/** Whether a layout region is a valid target for a cross-reference kind. */
function matchesCrossrefKind(
	kind: CrossrefKind,
	layoutKind: PdfLayoutKind,
): boolean {
	switch (kind) {
		case "figure":
			return isFigureLayoutKind(layoutKind);
		case "table":
			return isTableLayoutKind(layoutKind);
		case "equation":
			return isFormulaLayoutKind(layoutKind);
		case "algorithm":
			return isAlgorithmLayoutKind(layoutKind);
	}
}

/** Vertical centre of a normalized region. */
function centreY(region: PdfLayoutRegion): number {
	return region.bbox.y + region.bbox.h / 2;
}

/**
 * Pick the layout region a cross-reference points at, or null when the target
 * page has no region of that kind.
 */
export function pickCrossrefRegion(
	regions: readonly PdfLayoutRegion[],
	pageIndex: number,
	pdfY: number,
	pageHeightPt: number | null,
	kind: CrossrefKind,
): PdfLayoutRegion | null {
	const candidates = hoverableLayoutRegionsOnPage(regions, pageIndex).filter(
		(region) => matchesCrossrefKind(kind, region.kind),
	);
	if (candidates.length === 0) return null;
	if (candidates.length === 1) return candidates[0] ?? null;

	// No page height → cannot place the anchor; prefer the largest float.
	if (!pageHeightPt || pageHeightPt <= 0) {
		return candidates.reduce((best, region) =>
			bboxArea(region.bbox) > bboxArea(best.bbox) ? region : best,
		);
	}

	const normY = clamp01(1 - pdfY / pageHeightPt);
	const containing = candidates.filter(
		(region) =>
			normY >= region.bbox.y && normY <= region.bbox.y + region.bbox.h,
	);
	const pool = containing.length > 0 ? containing : candidates;
	return pool.reduce((best, region) =>
		Math.abs(centreY(region) - normY) < Math.abs(centreY(best) - normY)
			? region
			: best,
	);
}
