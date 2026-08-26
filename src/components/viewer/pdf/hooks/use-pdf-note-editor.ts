/**
 * The 批注 note editor for a text highlight or visual note.
 *
 * Wide host: edit in place on the right-rail comment card (Notion-style).
 * Narrow host / gutter pin: the floating AnnotationEditor card. The
 * annotations panel and a gutter pin both open it by annotation id, so it
 * reads the annotation straight from the plugin scope and claims the shared
 * hover surface so a pin leave cannot close it mid-edit.
 */

import type { useAnnotationCapability } from "@embedpdf/plugin-annotation/react";
import {
	type Dispatch,
	type RefObject,
	type SetStateAction,
	useCallback,
	useState,
} from "react";
import { pageElByIndex, rectRightScreen } from "@/components/viewer/pdf/coords";
import type {
	EditorState,
	PageAnnotationComment,
	RailEditState,
} from "@/components/viewer/pdf/types";
import {
	highlightColorOf,
	highlightQuoteOf,
	isHighlightObject,
} from "@/lib/pdf/highlight/annotation-store";

type AnnotationCapabilityProvides = ReturnType<
	typeof useAnnotationCapability
>["provides"];

export type UsePdfNoteEditorOptions = {
	docId: string;
	/** EmbedPDF capability; owned by `PdfViewerInner` (plugin context). */
	annotationCap: AnnotationCapabilityProvides;
	hostRef: RefObject<HTMLDivElement | null>;
	zoomRef: RefObject<number>;
	/** When true, prefer in-place rail edit over the floating editor. */
	commentRailEnabled: boolean;
	/** Normalized Y of a highlight (0–1) for a pending rail card. */
	anchorYForHighlight: (id: string) => number;
	/** Cards cluster: opening claims the hover surface, like `openCard` does. */
	cancelHoverHide: () => void;
	cardHoverSurfaceRef: RefObject<boolean>;
	/** Highlights cluster writers. */
	updateHighlightComment: (
		pageIndex: number,
		id: string,
		comment: string,
	) => void;
	deleteHighlightAnnotation: (pageIndex: number, id: string) => void;
	/** Visual-mark comment writer / deleter (rail cards). */
	updateVisualComment: (id: string, comment: string) => void;
	deleteVisualTraceById: (id: string) => void;
};

export type PdfNoteEditor = {
	editor: EditorState | null;
	/** In-place rail edit; null when idle or using the floating editor. */
	railEdit: RailEditState | null;
	/** Open for a highlight that was just created from the selection menu. */
	setEditor: Dispatch<SetStateAction<EditorState | null>>;
	/** Open the rail editor (new note from the selection menu, or a rail card). */
	beginRailEdit: (state: RailEditState) => void;
	/** Open for an existing highlight (annotations panel row or gutter pin). */
	openEditorForAnnotation: (id: string) => void;
	closeEditor: () => void;
	closeRailEdit: () => void;
	saveEditor: (text: string) => void;
	saveRailEdit: (comment: PageAnnotationComment, text: string) => void;
	/** Header delete: remove the highlight and close. */
	deleteEditorAnnotation: () => void;
	deleteRailComment: (comment: PageAnnotationComment) => void;
};

export function usePdfNoteEditor({
	docId,
	annotationCap,
	hostRef,
	zoomRef,
	commentRailEnabled,
	anchorYForHighlight,
	cancelHoverHide,
	cardHoverSurfaceRef,
	updateHighlightComment,
	deleteHighlightAnnotation,
	updateVisualComment,
	deleteVisualTraceById,
}: UsePdfNoteEditorOptions): PdfNoteEditor {
	const [editor, setEditor] = useState<EditorState | null>(null);
	const [railEdit, setRailEdit] = useState<RailEditState | null>(null);

	const beginRailEdit = useCallback((state: RailEditState) => {
		setEditor(null);
		setRailEdit((current) => (current?.id === state.id ? current : state));
	}, []);

	const closeRailEdit = useCallback(() => setRailEdit(null), []);

	const openEditorForAnnotation = useCallback(
		(id: string) => {
			const obj = annotationCap
				?.forDocument(docId)
				.getAnnotationById(id)?.object;
			if (!obj || !isHighlightObject(obj)) return;
			const comment = obj.contents?.trim() ?? "";
			if (commentRailEnabled) {
				beginRailEdit({
					id,
					pageIndex: obj.pageIndex,
					kind: "highlight",
					comment,
					quote: highlightQuoteOf(obj),
					color: highlightColorOf(obj),
					anchorY: anchorYForHighlight(id),
				});
				return;
			}
			const pageEl = pageElByIndex(hostRef.current, obj.pageIndex);
			if (!pageEl) return;
			// Same sticky-hover contract as openCard — pin leave must not close
			// the note editor while the user is moving onto / into the modal.
			cancelHoverHide();
			cardHoverSurfaceRef.current = true;
			setRailEdit(null);
			setEditor({
				screen: rectRightScreen(pageEl, obj.rect, zoomRef.current),
				pageIndex: obj.pageIndex,
				id,
				comment,
			});
		},
		[
			annotationCap,
			docId,
			commentRailEnabled,
			beginRailEdit,
			anchorYForHighlight,
			cancelHoverHide,
			cardHoverSurfaceRef,
			hostRef,
			zoomRef,
		],
	);

	const closeEditor = useCallback(() => setEditor(null), []);

	const saveEditor = useCallback(
		(text: string) => {
			if (!editor) return;
			updateHighlightComment(editor.pageIndex, editor.id, text);
			setEditor(null);
		},
		[editor, updateHighlightComment],
	);

	const saveRailEdit = useCallback(
		(comment: PageAnnotationComment, text: string) => {
			if (comment.kind === "visual") {
				updateVisualComment(comment.id, text);
			} else {
				updateHighlightComment(comment.pageIndex, comment.id, text);
			}
			setRailEdit((current) => (current?.id === comment.id ? null : current));
		},
		[updateHighlightComment, updateVisualComment],
	);

	const deleteEditorAnnotation = useCallback(() => {
		if (!editor) return;
		deleteHighlightAnnotation(editor.pageIndex, editor.id);
		setEditor(null);
	}, [editor, deleteHighlightAnnotation]);

	const deleteRailComment = useCallback(
		(comment: PageAnnotationComment) => {
			if (comment.kind === "visual") {
				deleteVisualTraceById(comment.id);
			} else {
				deleteHighlightAnnotation(comment.pageIndex, comment.id);
			}
			setRailEdit((current) => (current?.id === comment.id ? null : current));
		},
		[deleteHighlightAnnotation, deleteVisualTraceById],
	);

	return {
		editor,
		railEdit,
		setEditor,
		beginRailEdit,
		openEditorForAnnotation,
		closeEditor,
		closeRailEdit,
		saveEditor,
		saveRailEdit,
		deleteEditorAnnotation,
		deleteRailComment,
	};
}

export function railEditFromComment(
	comment: PageAnnotationComment,
): RailEditState {
	return {
		id: comment.id,
		pageIndex: comment.pageIndex,
		kind: comment.kind,
		comment: comment.comment,
		quote: comment.quote,
		color: comment.color,
		anchorY: comment.anchorY,
	};
}
