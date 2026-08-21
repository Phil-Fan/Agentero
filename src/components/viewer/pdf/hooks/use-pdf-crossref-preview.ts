/**
 * Cross-reference (`\ref`) hover preview for the EmbedPDF viewer: hovering a
 * "Fig. 3" / "Table 1" / "Eq. (2)" link shows a crop of the figure / table /
 * equation it points at.
 *
 * Resolution is exact through the hyperref cross-reference destination map
 * (`figure.*` / `table.*` / `equation.*` / `algorithm.*` — see
 * `lib/pdf/citation-dest-keys`) combined with the document's layout regions:
 * the destination coordinate gives the target page + kind, layout analysis
 * gives the region bbox, and the region is cropped on demand. Links the map or
 * layout cannot resolve show nothing.
 *
 * Its own hook because the preview is a self-contained hover state machine that
 * runs an async crop — kept separate from `usePdfCitations` (citations resolve
 * synchronously to a sidecar entry). A coordinate is either a `cite.*` or a
 * cross-reference destination, never both, so both hover handlers can run on
 * the same link and at most one card appears.
 */

import type { PdfEngine, PdfLinkAnnoObject } from "@embedpdf/models";
import type { useDocumentManagerCapability } from "@embedpdf/plugin-document-manager/react";
import {
	type RefObject,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import { pageElByIndex, rectRightScreen } from "@/components/viewer/pdf/coords";
import { getLinkDestination } from "@/components/viewer/pdf/layers/citation-links";
import { renderPdfRegionPromptImage } from "@/components/viewer/pdf/region-crop";
import type { CrossrefPreviewState } from "@/components/viewer/pdf/types";
import { errorText } from "@/lib/core/error";
import { logger } from "@/lib/core/logger";
import {
	type CrossrefDestMap,
	citationDestKey,
} from "@/lib/pdf/citation-dest-keys";
import { loadPdfDestMaps } from "@/lib/pdf/citation-dest-map";
import { pickCrossrefRegion } from "@/lib/pdf/crossref-resolve";
import { getLayoutDocumentResult } from "@/lib/pdf/layout";

/** Grace period so the pointer can travel from the link into the card. */
const CROSSREF_HIDE_MS = 250;
/** Upper bound before the deferred dest-map build runs anyway. */
const DEST_MAP_IDLE_TIMEOUT_MS = 2000;
/** Fallback delay when `requestIdleCallback` is unavailable (WebKit). */
const DEST_MAP_FALLBACK_DELAY_MS = 500;
/** Longest edge of the preview crop (px). */
const CROSSREF_CROP_MAX_EDGE = 520;

function scheduleIdle(fn: () => void): () => void {
	if (typeof requestIdleCallback === "function") {
		const id = requestIdleCallback(fn, { timeout: DEST_MAP_IDLE_TIMEOUT_MS });
		return () => cancelIdleCallback(id);
	}
	const id = setTimeout(fn, DEST_MAP_FALLBACK_DELAY_MS);
	return () => clearTimeout(id);
}

type DocumentManagerCapability = ReturnType<
	typeof useDocumentManagerCapability
>["provides"];

export type UsePdfCrossrefPreviewOptions = {
	docId: string;
	hostRef: RefObject<HTMLDivElement | null>;
	/** Current zoom, mirrored so the preview anchor never re-creates handlers. */
	zoomRef: RefObject<number>;
	/** Absolute paper folder; used to read PDF bytes for the dest map. */
	paperAbsPath: string | null;
	/** PDF bytes the viewer already holds; reused (copied) by the map build. */
	sourceBytes?: ArrayBuffer | null;
	/** Engine + document manager, for cropping the resolved region. */
	engineRef: RefObject<PdfEngine | null>;
	docCapRef: RefObject<DocumentManagerCapability>;
};

export type PdfCrossrefPreview = {
	crossrefPreview: CrossrefPreviewState | null;
	cancelCrossrefHide: () => void;
	scheduleCrossrefHide: () => void;
	handleCrossrefLinkHover: (link: PdfLinkAnnoObject | null) => void;
};

export function usePdfCrossrefPreview({
	docId,
	hostRef,
	zoomRef,
	paperAbsPath,
	sourceBytes = null,
	engineRef,
	docCapRef,
}: UsePdfCrossrefPreviewOptions): PdfCrossrefPreview {
	const [crossrefPreview, setCrossrefPreview] =
		useState<CrossrefPreviewState | null>(null);
	const hideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
	/** hyperref cross-reference destinations of the open PDF, by coords. */
	const crossrefMapRef = useRef<CrossrefDestMap | null>(null);
	/** Monotonic token so a stale crop never lands over a newer hover. */
	const renderTokenRef = useRef(0);

	const sourceBytesRef = useRef<ArrayBuffer | null>(sourceBytes);
	sourceBytesRef.current = sourceBytes;

	useEffect(() => {
		crossrefMapRef.current = null;
		if (!paperAbsPath) return;
		let cancelled = false;
		const cancelIdle = scheduleIdle(() => {
			void loadPdfDestMaps({
				paperAbsPath,
				viewerBytes: sourceBytesRef.current,
			})
				.then((maps) => {
					if (!cancelled && maps) crossrefMapRef.current = maps.crossrefs;
				})
				.catch((error: unknown) => {
					logger.warn("crossref dest map failed", {
						error: errorText(error),
					});
				});
		});
		return () => {
			cancelled = true;
			cancelIdle();
		};
	}, [paperAbsPath]);

	// Reset the preview when the active PDF document changes.
	// biome-ignore lint/correctness/useExhaustiveDependencies: docId is the effect trigger, not a value read inside the effect.
	useEffect(() => {
		setCrossrefPreview(null);
	}, [docId]);

	const cancelCrossrefHide = useCallback(() => {
		if (!hideTimerRef.current) return;
		clearTimeout(hideTimerRef.current);
		hideTimerRef.current = null;
	}, []);

	const scheduleCrossrefHide = useCallback(() => {
		cancelCrossrefHide();
		hideTimerRef.current = setTimeout(() => {
			hideTimerRef.current = null;
			setCrossrefPreview(null);
		}, CROSSREF_HIDE_MS);
	}, [cancelCrossrefHide]);

	const handleCrossrefLinkHover = useCallback(
		(link: PdfLinkAnnoObject | null) => {
			if (!link) {
				scheduleCrossrefHide();
				return;
			}
			const destination = getLinkDestination(link.target);
			const kind = destination
				? crossrefMapRef.current?.get(
						citationDestKey(destination.pageIndex, destination.pdfY),
					)
				: undefined;
			if (!destination || !kind) {
				scheduleCrossrefHide();
				return;
			}
			const regions = getLayoutDocumentResult(docId)?.regions ?? [];
			const document = docCapRef.current?.getDocument(docId) ?? null;
			const pageHeightPt =
				document?.pages[destination.pageIndex]?.size.height ?? null;
			const region = pickCrossrefRegion(
				regions,
				destination.pageIndex,
				destination.pdfY,
				pageHeightPt,
				kind,
			);
			if (!region) {
				scheduleCrossrefHide();
				return;
			}
			cancelCrossrefHide();
			const pageEl = pageElByIndex(hostRef.current, link.pageIndex);
			if (!pageEl) return;
			setCrossrefPreview({
				screen: rectRightScreen(pageEl, link.rect, zoomRef.current),
				kind,
				page: region.pageIndex + 1,
				region: region.bbox,
				image: null,
			});

			// Crop the region asynchronously; drop the result if a newer hover
			// (or a document close) superseded it.
			const token = ++renderTokenRef.current;
			const engine = engineRef.current;
			if (!engine || !document) return;
			void renderPdfRegionPromptImage({
				engine,
				document,
				pageIndex: region.pageIndex,
				region: region.bbox,
				maxEdgePx: CROSSREF_CROP_MAX_EDGE,
			})
				.then((image) => {
					if (renderTokenRef.current !== token) return;
					setCrossrefPreview((prev) =>
						prev && prev.image === null ? { ...prev, image } : prev,
					);
				})
				.catch(() => {});
		},
		[
			docId,
			hostRef,
			zoomRef,
			engineRef,
			docCapRef,
			cancelCrossrefHide,
			scheduleCrossrefHide,
		],
	);

	// Clean up the hide timer when the document changes or unmounts.
	// biome-ignore lint/correctness/useExhaustiveDependencies: docId is the effect trigger, not a value read inside the cleanup.
	useEffect(
		() => () => {
			if (hideTimerRef.current) clearTimeout(hideTimerRef.current);
		},
		[docId],
	);

	return {
		crossrefPreview,
		cancelCrossrefHide,
		scheduleCrossrefHide,
		handleCrossrefLinkHover,
	};
}
