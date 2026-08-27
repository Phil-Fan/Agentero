/**
 * In-text citation / internal PDF link behaviour for the EmbedPDF viewer:
 * activating a link (GoTo destination → scroll, URI → system browser) and the
 * hover reference card.
 *
 * The card shows the reference a citation points at. Resolution order:
 * 1. Unambiguous dest coordinate → key (hyperref `cite.<bibtexKey>` /XYZ).
 * 2. Link annotation rect → dest key (`mk:ref12`) when ACS `/FitR`
 *    bibliography entries collide on one page.
 * 3. Key → sidecar via `rawKey`, or ACS `mk:refN` → `id: "ref-N"`.
 * Links that cannot be resolved show nothing.
 *
 * Its own hook because the preview is a self-contained hover state machine —
 * a short hide delay lets the pointer travel from the link into the card.
 * Nothing else in the viewer reads it.
 *
 * The per-page link hit-target map (`citationLinks` in the highlights cluster)
 * is *not* owned here: it is a by-product of the annotation rebuild, so it
 * stays with its single writer and is passed straight into the page layers.
 */

import type { PdfLinkAnnoObject } from "@embedpdf/models";
import type { useAnnotationCapability } from "@embedpdf/plugin-annotation/react";
import {
	type RefObject,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import { pageElByIndex, rectRightScreen } from "@/components/viewer/pdf/coords";
import { getLinkDestination } from "@/components/viewer/pdf/layers/citation-links";
import type { CitationPreviewState } from "@/components/viewer/pdf/types";
import { useCitationImport } from "@/hooks/use-citation-import";
import { usePaperRefsSidecar } from "@/hooks/use-paper-refs-sidecar";
import { errorText } from "@/lib/core/error";
import { logger } from "@/lib/core/logger";
import { openExternalUrl } from "@/lib/core/open-external";
import type { Citation } from "@/lib/paper/refs";
import {
	type CitationDestKeyMap,
	type CitationLinkKeyList,
	citationDestKey,
	citationSidecarKeysForDest,
	expandCitationLinkCluster,
	matchCitationLinkKey,
} from "@/lib/pdf/citation-dest-keys";
import { loadPdfDestMaps } from "@/lib/pdf/citation-dest-map";

/** Grace period so the pointer can travel from the link into the card. */
const CITATION_HIDE_MS = 250;

/** Upper bound before the deferred dest-key map build runs anyway. */
const DEST_MAP_IDLE_TIMEOUT_MS = 2000;
/** Fallback delay when `requestIdleCallback` is unavailable (WebKit). */
const DEST_MAP_FALLBACK_DELAY_MS = 500;

/**
 * Run `fn` when the main thread is idle (bounded), so the map build never
 * competes with the PDF-open critical path (first paint, scroll, selection).
 * Returns a canceller.
 */
function scheduleIdle(fn: () => void): () => void {
	if (typeof requestIdleCallback === "function") {
		const id = requestIdleCallback(fn, { timeout: DEST_MAP_IDLE_TIMEOUT_MS });
		return () => cancelIdleCallback(id);
	}
	const id = setTimeout(fn, DEST_MAP_FALLBACK_DELAY_MS);
	return () => clearTimeout(id);
}

type AnnotationCapabilityProvides = ReturnType<
	typeof useAnnotationCapability
>["provides"];

export type UsePdfCitationsOptions = {
	docId: string;
	/** EmbedPDF capability; owned by `PdfViewerInner` (plugin context). */
	annotationCap: AnnotationCapabilityProvides;
	hostRef: RefObject<HTMLDivElement | null>;
	/** Current zoom, mirrored so the preview anchor never re-creates handlers. */
	zoomRef: RefObject<number>;
	/** Vault root; with `paperPath` enables sidecar-backed preview matching. */
	vaultPath: string | null;
	/** Vault-relative paper folder of the open PDF, or null when not a paper. */
	paperPath: string | null;
	/** Absolute paper folder; used to read PDF bytes for the cite-key map. */
	paperAbsPath: string | null;
	/**
	 * PDF bytes the viewer already holds (`tab.pdfBytes`). Reused (copied) by
	 * the cite-key map build so opening a paper never reads the PDF twice.
	 */
	sourceBytes?: ArrayBuffer | null;
};

export type PdfCitations = {
	citationPreview: CitationPreviewState | null;
	cancelCitationHide: () => void;
	scheduleCitationHide: () => void;
	handleCitationLinkActivate: (link: PdfLinkAnnoObject) => void;
	handleCitationLinkHover: (link: PdfLinkAnnoObject | null) => void;
	/** Library-import surface for the hover card; null when not a vault paper. */
	citationImport: {
		folders: string[];
		lastImportParentDir: string;
		importingId: string | null;
		importCitation: (citation: Citation, parentDir: string) => void;
	} | null;
};

/**
 * Match a PDF destination key to a sidecar citation. Hyperref keys hit
 * `rawKey`; ACS `mk:refN` hits sidecar `id: "ref-N"` (no rawKey on S2 parses).
 */
function findCitationByDestKey(
	destKey: string,
	citations: Citation[],
): Citation | undefined {
	const candidates = new Set(citationSidecarKeysForDest(destKey));
	return citations.find(
		(citation) =>
			(citation.rawKey != null && candidates.has(citation.rawKey)) ||
			candidates.has(citation.id),
	);
}

/**
 * Which reference(s) a citation link points at. Prefers unambiguous dest-coord
 * keys (hyperref `/XYZ`); falls back to the link-annotation dest name when ACS
 * `/FitR` bibliography entries collide. Nearby ACS superscripts are clustered
 * so `14-18` expands to refs 14…18 (comma-separated neighbours stay separate).
 * Returns an empty list when nothing resolves, so the card does not appear.
 */
function resolveCitations(
	link: PdfLinkAnnoObject,
	destKeys: CitationDestKeyMap | null,
	citationLinks: CitationLinkKeyList | null,
	citations: Citation[],
): Citation[] {
	const destination = getLinkDestination(link.target);
	if (destination) {
		const byCoord = destKeys?.get(
			citationDestKey(destination.pageIndex, destination.pdfY),
		);
		if (byCoord) {
			const matched = findCitationByDestKey(byCoord, citations);
			if (matched) return [matched];
		}
	}

	const clusterKeys = expandCitationLinkCluster(
		citationLinks,
		link.pageIndex,
		link.rect,
	);
	if (clusterKeys.length === 0) {
		const byLink = matchCitationLinkKey(
			citationLinks,
			link.pageIndex,
			link.rect,
		);
		if (!byLink) return [];
		const matched = findCitationByDestKey(byLink, citations);
		return matched ? [matched] : [];
	}

	const out: Citation[] = [];
	const seen = new Set<string>();
	for (const key of clusterKeys) {
		const matched = findCitationByDestKey(key, citations);
		if (!matched || seen.has(matched.id)) continue;
		seen.add(matched.id);
		out.push(matched);
	}
	return out;
}

export function usePdfCitations({
	docId,
	annotationCap,
	hostRef,
	zoomRef,
	vaultPath,
	paperPath,
	paperAbsPath,
	sourceBytes = null,
}: UsePdfCitationsOptions): PdfCitations {
	const [citationPreview, setCitationPreview] =
		useState<CitationPreviewState | null>(null);
	const citationHideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const { sidecar, setSidecar } = usePaperRefsSidecar(vaultPath, paperPath);
	/** Mirrored so the hover callback identity does not change per sidecar load. */
	const citationsRef = useRef(sidecar?.citations ?? []);
	citationsRef.current = sidecar?.citations ?? [];

	// ---- Library import (shared with the References panel) ----
	const { folders, lastImportParentDir, importingId, importCitation } =
		useCitationImport(vaultPath, paperPath, setSidecar);

	const citationImport =
		vaultPath && paperPath
			? { folders, lastImportParentDir, importingId, importCitation }
			: null;

	/** hyperref `cite.<key>` destinations of the open PDF, by destination coords. */
	const destKeyMapRef = useRef<CitationDestKeyMap | null>(null);
	/**
	 * Link annotation rect → citation dest key. Used when ACS `/FitR`
	 * bibliography destinations collide and the coord map is empty.
	 */
	const citationLinksRef = useRef<CitationLinkKeyList | null>(null);
	/** Mirrored so a late bytes prop never re-triggers the build effect. */
	const sourceBytesRef = useRef<ArrayBuffer | null>(sourceBytes);
	sourceBytesRef.current = sourceBytes;
	useEffect(() => {
		destKeyMapRef.current = null;
		citationLinksRef.current = null;
		if (!paperAbsPath) return;
		let cancelled = false;
		// Deferred to idle, parsed in a worker, memoized per PDF — the build
		// never blocks the open-PDF critical path (hover previews simply do not
		// resolve until the map is ready).
		const cancelIdle = scheduleIdle(() => {
			void loadPdfDestMaps({
				paperAbsPath,
				viewerBytes: sourceBytesRef.current,
			})
				.then((maps) => {
					if (!cancelled && maps) {
						destKeyMapRef.current = maps.cites;
						citationLinksRef.current = maps.citationLinks;
					}
				})
				.catch((error: unknown) => {
					// Non-fatal: hover previews simply do not resolve.
					logger.warn("citation dest key map failed", {
						error: errorText(error),
					});
				});
		});
		return () => {
			cancelled = true;
			cancelIdle();
		};
	}, [paperAbsPath]);

	// Reset the hover preview when the active PDF document changes.
	// biome-ignore lint/correctness/useExhaustiveDependencies: docId is the effect trigger, not a value read inside the effect.
	useEffect(() => {
		setCitationPreview(null);
	}, [docId]);

	const cancelCitationHide = useCallback(() => {
		if (!citationHideTimerRef.current) return;
		clearTimeout(citationHideTimerRef.current);
		citationHideTimerRef.current = null;
	}, []);

	const scheduleCitationHide = useCallback(() => {
		cancelCitationHide();
		citationHideTimerRef.current = setTimeout(() => {
			citationHideTimerRef.current = null;
			setCitationPreview(null);
		}, CITATION_HIDE_MS);
	}, [cancelCitationHide]);

	/** GoTo/destination → smooth scroll (annotation plugin); URI → browser. */
	const handleCitationLinkActivate = useCallback(
		(link: PdfLinkAnnoObject) => {
			const target = link.target;
			if (!target || !annotationCap) return;
			annotationCap
				.navigateTarget(target, docId)
				.toPromise()
				.then((result) => {
					if (result.outcome === "uri") openExternalUrl(result.uri);
				})
				.catch(() => {});
		},
		[annotationCap, docId],
	);

	const handleCitationLinkHover = useCallback(
		(link: PdfLinkAnnoObject | null) => {
			if (!link) {
				scheduleCitationHide();
				return;
			}
			const matched = resolveCitations(
				link,
				destKeyMapRef.current,
				citationLinksRef.current,
				citationsRef.current,
			);
			if (matched.length === 0) {
				scheduleCitationHide();
				return;
			}
			cancelCitationHide();
			const pageEl = pageElByIndex(hostRef.current, link.pageIndex);
			if (!pageEl) return;
			setCitationPreview({
				screen: rectRightScreen(pageEl, link.rect, zoomRef.current),
				matched,
			});
		},
		[scheduleCitationHide, cancelCitationHide, hostRef, zoomRef],
	);

	// Clean up the citation preview hide timer when the document changes or unmounts.
	// biome-ignore lint/correctness/useExhaustiveDependencies: docId is the effect trigger, not a value read inside the cleanup.
	useEffect(
		() => () => {
			if (citationHideTimerRef.current) {
				clearTimeout(citationHideTimerRef.current);
			}
		},
		[docId],
	);

	return {
		citationPreview,
		cancelCitationHide,
		scheduleCitationHide,
		handleCitationLinkActivate,
		handleCitationLinkHover,
		citationImport,
	};
}
