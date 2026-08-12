/**
 * In-text citation / internal PDF link behaviour for the EmbedPDF viewer:
 * activating a link (GoTo destination → scroll, URI → system browser) and the
 * hover preview card.
 *
 * The card shows two things: the text merged from the destination page (noisy,
 * geometric) and the reference it belongs to. The reference is resolved exactly
 * through the hyperref `cite.<bibtexKey>` destination map when the PDF has one
 * (95% of in-text links across 31 sampled papers, no wrong hits), and falls back
 * to fuzzy text matching against the sidecar otherwise.
 *
 * Its own hook because the preview is a self-contained hover state machine — a
 * sequence guard for out-of-order resolves plus a short hide delay so the
 * pointer can travel from the link into the card. Nothing else in the viewer
 * reads it.
 *
 * The per-page link map itself (`citationLinks`) is *not* owned here: it is a
 * by-product of the annotation rebuild in the highlights cluster, so it stays
 * with its single writer and is passed straight into the page layers.
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
import { useDestinationPreviewResolver } from "@/components/viewer/pdf/layers/citation-links";
import type { CitationPreviewState } from "@/components/viewer/pdf/types";
import { usePaperRefsSidecar } from "@/hooks/use-paper-refs-sidecar";
import { logger } from "@/lib/core/logger";
import { openExternalUrl } from "@/lib/core/open-external";
import { findLocalPdfPath, localFileToArrayBuffer } from "@/lib/paper";
import { matchCitationByText } from "@/lib/paper/citation-match";
import type { Citation } from "@/lib/paper/refs";
import {
	buildCitationDestKeyMap,
	type CitationDestKeyMap,
	citationDestKey,
} from "@/lib/pdf/citation-dest-keys";

/** Grace period so the pointer can travel from the link into the card. */
const CITATION_HIDE_MS = 250;

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
};

export type PdfCitations = {
	citationPreview: CitationPreviewState | null;
	cancelCitationHide: () => void;
	scheduleCitationHide: () => void;
	handleCitationLinkActivate: (link: PdfLinkAnnoObject) => void;
	handleCitationLinkHover: (link: PdfLinkAnnoObject | null) => void;
};

/**
 * Which reference a citation link points at. The hyperref key is exact, so it
 * wins; fuzzy text matching only covers PDFs without cite destinations (or
 * bibliographies whose keys never made it into the sidecar).
 */
function resolveCitation(
	pageIndex: number,
	pdfY: number,
	extractedText: string,
	destKeys: CitationDestKeyMap | null,
	citations: Citation[],
): Citation | undefined {
	const key = destKeys?.get(citationDestKey(pageIndex, pdfY));
	if (key) {
		const exact = citations.find((citation) => citation.rawKey === key);
		if (exact) return exact;
	}
	return matchCitationByText(extractedText, citations) ?? undefined;
}

export function usePdfCitations({
	docId,
	annotationCap,
	hostRef,
	zoomRef,
	vaultPath,
	paperPath,
	paperAbsPath,
}: UsePdfCitationsOptions): PdfCitations {
	const [citationPreview, setCitationPreview] =
		useState<CitationPreviewState | null>(null);
	const citationHideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const resolveDestinationPreview = useDestinationPreviewResolver(docId);
	/** Bumped per hover so a late resolve cannot revive a stale preview. */
	const linkHoverSeqRef = useRef(0);
	const { sidecar } = usePaperRefsSidecar(vaultPath, paperPath);
	/** Mirrored so the hover callback identity does not change per sidecar load. */
	const citationsRef = useRef(sidecar?.citations ?? []);
	citationsRef.current = sidecar?.citations ?? [];

	/** hyperref `cite.<key>` destinations of the open PDF, by destination coords. */
	const destKeyMapRef = useRef<CitationDestKeyMap | null>(null);
	useEffect(() => {
		destKeyMapRef.current = null;
		if (!paperAbsPath) return;
		let cancelled = false;
		void (async () => {
			try {
				const pdfPath = await findLocalPdfPath(paperAbsPath);
				if (!pdfPath || cancelled) return;
				const bytes = await localFileToArrayBuffer(pdfPath);
				if (!bytes || cancelled) return;
				const map = await buildCitationDestKeyMap(bytes);
				if (!cancelled) destKeyMapRef.current = map;
			} catch (error) {
				// Non-fatal: hover falls back to fuzzy text matching.
				logger.warn("citation dest key map failed", {
					error: error instanceof Error ? error.message : String(error),
				});
			}
		})();
		return () => {
			cancelled = true;
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
			const seq = ++linkHoverSeqRef.current;
			if (!link) {
				scheduleCitationHide();
				return;
			}
			cancelCitationHide();
			setCitationPreview(null);
			void resolveDestinationPreview(link).then((preview) => {
				if (linkHoverSeqRef.current !== seq || !preview) return;
				const pageEl = pageElByIndex(hostRef.current, link.pageIndex);
				if (!pageEl) return;
				setCitationPreview({
					screen: rectRightScreen(pageEl, link.rect, zoomRef.current),
					previewText: preview.text,
					matched: resolveCitation(
						preview.pageIndex,
						preview.pdfY,
						preview.text,
						destKeyMapRef.current,
						citationsRef.current,
					),
				});
			});
		},
		[
			resolveDestinationPreview,
			scheduleCitationHide,
			cancelCitationHide,
			hostRef,
			zoomRef,
		],
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
	};
}
