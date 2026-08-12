/**
 * In-text citation / internal PDF link behaviour for the EmbedPDF viewer:
 * activating a link (GoTo destination → scroll, URI → system browser) and the
 * hover preview card that shows the destination text (usually the bibliography
 * entry). The extracted text is fuzzy-matched against the paper's
 * `agentero-cite.json` sidecar; the clean parsed entry wins when found.
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
import { openExternalUrl } from "@/lib/core/open-external";
import { matchCitationByText } from "@/lib/paper/citation-match";

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
};

export type PdfCitations = {
	citationPreview: CitationPreviewState | null;
	cancelCitationHide: () => void;
	scheduleCitationHide: () => void;
	handleCitationLinkActivate: (link: PdfLinkAnnoObject) => void;
	handleCitationLinkHover: (link: PdfLinkAnnoObject | null) => void;
};

export function usePdfCitations({
	docId,
	annotationCap,
	hostRef,
	zoomRef,
	vaultPath,
	paperPath,
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
			void resolveDestinationPreview(link).then((previewText) => {
				if (linkHoverSeqRef.current !== seq || !previewText) return;
				const pageEl = pageElByIndex(hostRef.current, link.pageIndex);
				if (!pageEl) return;
				// The geometric extraction is noisy; prefer the clean sidecar entry
				// text of the best match (and carry its id for panel linkage).
				const matched = matchCitationByText(previewText, citationsRef.current);
				setCitationPreview({
					screen: rectRightScreen(pageEl, link.rect, zoomRef.current),
					previewText: matched?.raw ?? matched?.metadata.title ?? previewText,
					citationId: matched?.id,
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
