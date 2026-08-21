/**
 * Region-crop draft card state for the PDF viewer.
 *
 * The card's *content* belongs to {@link usePdfVisualMarks}; the state and the
 * region anchor live here because both region framing (⌘. marquee) and the
 * layout hit targets open the same card.
 */

import { type RefObject, useCallback, useState } from "react";
import { pageElByIndex } from "@/components/viewer/pdf/coords";
import type {
	ScreenPoint,
	VisualDraftEditorState,
} from "@/components/viewer/pdf/types";
import type { PdfAskNormalizedRect } from "@/lib/pdf/ask/types";

export type UsePdfVisualDraftOptions = {
	hostRef: RefObject<HTMLDivElement | null>;
};

export type PdfVisualDraft = {
	visualDraftEditor: VisualDraftEditorState | null;
	openVisualDraftEditor: (draft: VisualDraftEditorState) => void;
	closeVisualDraftEditor: () => void;
	/** Screen anchor beside a page-normalized region (draft card placement). */
	screenPointForRegion: (
		pageIndex0: number,
		region: PdfAskNormalizedRect,
	) => ScreenPoint;
};

export function usePdfVisualDraft({
	hostRef,
}: UsePdfVisualDraftOptions): PdfVisualDraft {
	const [visualDraftEditor, setVisualDraftEditor] =
		useState<VisualDraftEditorState | null>(null);

	const openVisualDraftEditor = useCallback((draft: VisualDraftEditorState) => {
		setVisualDraftEditor(draft);
	}, []);

	const closeVisualDraftEditor = useCallback(() => {
		setVisualDraftEditor(null);
	}, []);

	/** Screen point near a layout bbox (right edge) for the draft card. */
	const screenPointForRegion = useCallback(
		(pageIndex0: number, region: PdfAskNormalizedRect) => {
			const pageEl = pageElByIndex(hostRef.current, pageIndex0);
			if (!pageEl) return { x: 120, y: 120 };
			const box = pageEl.getBoundingClientRect();
			return {
				x: box.left + (region.x + region.w) * box.width + 8,
				y: box.top + region.y * box.height,
			};
		},
		[hostRef],
	);

	return {
		visualDraftEditor,
		openVisualDraftEditor,
		closeVisualDraftEditor,
		screenPointForRegion,
	};
}
