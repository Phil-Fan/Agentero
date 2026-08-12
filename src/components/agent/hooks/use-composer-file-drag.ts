/**
 * Image drop highlight for the composer shell: nested enter/leave counter so
 * moving over chips/textarea does not flicker the drop ring.
 */
import type { DragEvent as ReactDragEvent } from "react";
import { useCallback, useEffect, useRef, useState } from "react";
import { dataTransferLooksLikeImages } from "@/lib/core/file-accept";

export function useComposerFileDrag() {
	const fileDragDepthRef = useRef(0);
	const [isFileDragOver, setIsFileDragOver] = useState(false);

	const resetFileDragHighlight = useCallback(() => {
		fileDragDepthRef.current = 0;
		setIsFileDragOver(false);
	}, []);

	const onFileDragEnter = useCallback((event: ReactDragEvent) => {
		// Only highlight when we can tell the payload is image-like.
		if (!dataTransferLooksLikeImages(event.dataTransfer)) return;
		event.preventDefault();
		fileDragDepthRef.current += 1;
		setIsFileDragOver(true);
	}, []);

	const onFileDragLeave = useCallback((event: ReactDragEvent) => {
		if (!dataTransferLooksLikeImages(event.dataTransfer)) return;
		fileDragDepthRef.current = Math.max(0, fileDragDepthRef.current - 1);
		if (fileDragDepthRef.current === 0) {
			setIsFileDragOver(false);
		}
	}, []);

	const onFileDragOver = useCallback((event: ReactDragEvent) => {
		if (!dataTransferLooksLikeImages(event.dataTransfer)) return;
		event.preventDefault();
		event.dataTransfer.dropEffect = "copy";
	}, []);

	const onFileDropHighlightEnd = useCallback(() => {
		// Drop fires before PromptInput's native listener consumes files; only clear UI.
		resetFileDragHighlight();
	}, [resetFileDragHighlight]);

	// Clear stuck highlight if the drag ends outside the composer (leave app, Esc, etc.).
	useEffect(() => {
		if (!isFileDragOver) return;
		const clear = () => resetFileDragHighlight();
		window.addEventListener("dragend", clear);
		window.addEventListener("drop", clear);
		return () => {
			window.removeEventListener("dragend", clear);
			window.removeEventListener("drop", clear);
		};
	}, [isFileDragOver, resetFileDragHighlight]);

	return {
		isFileDragOver,
		onFileDragEnter,
		onFileDragLeave,
		onFileDragOver,
		onFileDropHighlightEnd,
	};
}
