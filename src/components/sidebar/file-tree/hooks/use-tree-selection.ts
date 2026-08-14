/**
 * Multi-selection (Ctrl/Cmd/Shift click), row activation, and the
 * selection-scoped actions (batch delete, cut, paste).
 */
import { useCallback, useEffect, useState } from "react";
import { LIBRARY_VIRTUAL_PATH, TRASH_VIRTUAL_PATH } from "@/lib/paper/api";
import {
	isPlazaVirtualPath,
	type PlazaSource,
	plazaSourceForPath,
} from "@/lib/plaza";
import type { FileNode } from "@/lib/vault";
import { isVirtualTreePath } from "../tree-helpers";
import type { TreeCreateDraft, TreeRenameDraft } from "../types";

export type RowClickMods = { meta: boolean; ctrl: boolean; shift: boolean };

export type TreeSelection = {
	selected: Set<string>;
	clearSelection: () => void;
	handleSelectRow: (path: string, mods: RowClickMods) => void;
	orderedSelected: () => string[];
	/** Action target: the whole selection when the row is part of it. */
	pathsForAction: (path: string) => string[];
	runBatchDelete: () => void;
	cutSelected: () => void;
	pasteIntoSelected: () => void;
};

export function useTreeSelection({
	nodes,
	selectedPath,
	selectableOrder,
	byPath,
	createDraft,
	renameDraft,
	onSelectFile,
	onSelectLibrary,
	onSelectTrash,
	onSelectPlaza,
	onSelectPlazaSource,
	onExpandPath,
	onDeletePath,
	onDeletePaths,
	onCutPaths,
	onPasteInto,
}: {
	nodes: FileNode[];
	selectedPath: string | null;
	selectableOrder: string[];
	byPath: ReadonlyMap<string, FileNode>;
	createDraft: TreeCreateDraft | null;
	renameDraft?: TreeRenameDraft | null;
	onSelectFile: (node: FileNode) => void;
	onSelectLibrary?: () => void;
	onSelectTrash?: () => void;
	onSelectPlaza?: () => void;
	onSelectPlazaSource?: (source: PlazaSource) => void;
	onExpandPath?: (path: string) => void;
	onDeletePath?: (path: string) => void | Promise<void>;
	onDeletePaths?: (paths: string[]) => void | Promise<void>;
	onCutPaths?: (paths: string[]) => void;
	onPasteInto?: (targetPath: string) => void;
}): TreeSelection {
	const [selected, setSelected] = useState<Set<string>>(new Set());
	const [anchor, setAnchor] = useState<string | null>(null);

	// Reset the multi-selection whenever the tree changes (post delete/move).
	// biome-ignore lint/correctness/useExhaustiveDependencies: reset on nodes change
	useEffect(() => {
		setSelected(new Set());
		setAnchor(null);
	}, [nodes]);

	const clearSelection = useCallback(() => {
		setSelected(new Set());
		setAnchor(null);
	}, []);

	const openRow = useCallback(
		(path: string) => {
			if (path === LIBRARY_VIRTUAL_PATH) {
				onSelectLibrary?.();
				return;
			}
			if (path === TRASH_VIRTUAL_PATH) {
				onSelectTrash?.();
				return;
			}
			// Plaza paths are virtual: they have no FileNode to fall through to.
			if (isPlazaVirtualPath(path)) {
				const source = plazaSourceForPath(path);
				if (source) onSelectPlazaSource?.(source);
				else {
					// Row click opens the plaza home and reveals Cool Papers etc.
					// Chevron still toggles collapse independently.
					onExpandPath?.(path);
					onSelectPlaza?.();
				}
				return;
			}
			const node = byPath.get(path);
			if (!node) return;
			// Files, paper folders, and org folders (e.g. papers/nlp/pretrain) —
			// parent opens paper / scoped library via onSelectFile.
			onSelectFile(node);
		},
		[
			byPath,
			onSelectFile,
			onSelectLibrary,
			onSelectTrash,
			onSelectPlaza,
			onSelectPlazaSource,
			onExpandPath,
		],
	);

	const handleSelectRow = useCallback(
		(path: string, mods: RowClickMods) => {
			if (createDraft || renameDraft) return;
			if (isVirtualTreePath(path)) {
				clearSelection();
				openRow(path);
				return;
			}
			if (mods.shift && anchor) {
				const a = selectableOrder.indexOf(anchor);
				const b = selectableOrder.indexOf(path);
				if (a !== -1 && b !== -1) {
					const [lo, hi] = a <= b ? [a, b] : [b, a];
					setSelected(new Set(selectableOrder.slice(lo, hi + 1)));
					return;
				}
			}
			if (mods.meta || mods.ctrl) {
				setSelected((prev) => {
					const next = new Set(prev);
					// Fold the current open/anchored row into a fresh multi-selection
					// so the count matches the highlighted rows.
					if (
						next.size === 0 &&
						anchor &&
						anchor !== path &&
						selectableOrder.includes(anchor)
					) {
						next.add(anchor);
					}
					if (next.has(path)) next.delete(path);
					else next.add(path);
					return next;
				});
				setAnchor(path);
				return;
			}
			// Plain click: drop any multi-selection and open the row.
			setSelected(new Set());
			setAnchor(path);
			openRow(path);
		},
		[
			anchor,
			clearSelection,
			createDraft,
			renameDraft,
			openRow,
			selectableOrder,
		],
	);

	const orderedSelected = useCallback(
		() => selectableOrder.filter((p) => selected.has(p)),
		[selectableOrder, selected],
	);

	const pathsForAction = useCallback(
		(path: string): string[] =>
			selected.has(path) && selected.size > 0 ? orderedSelected() : [path],
		[selected, orderedSelected],
	);

	const runBatchDelete = useCallback(() => {
		const paths = orderedSelected();
		if (paths.length === 0) return;
		if (onDeletePaths) void onDeletePaths(paths);
		else if (onDeletePath && paths[0]) void onDeletePath(paths[0]);
	}, [orderedSelected, onDeletePaths, onDeletePath]);

	const cutSelected = useCallback(() => {
		const paths =
			selected.size > 0
				? orderedSelected()
				: selectedPath
					? [selectedPath]
					: [];
		if (paths.length > 0) {
			onCutPaths?.(paths);
		}
	}, [selected.size, selectedPath, orderedSelected, onCutPaths]);

	const pasteIntoSelected = useCallback(() => {
		if (selectedPath) {
			onPasteInto?.(selectedPath);
		}
	}, [selectedPath, onPasteInto]);

	// Delete / clear the multi-selection via the keyboard.
	useEffect(() => {
		if (selected.size === 0) return;
		const onKey = (e: KeyboardEvent) => {
			const el = e.target as HTMLElement | null;
			if (
				el &&
				(el.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(el.tagName))
			) {
				return;
			}
			if (e.key === "Escape") {
				clearSelection();
			} else if (
				e.key === "Delete" ||
				(e.key === "Backspace" && (e.metaKey || e.ctrlKey))
			) {
				e.preventDefault();
				runBatchDelete();
			}
		};
		window.addEventListener("keydown", onKey);
		return () => window.removeEventListener("keydown", onKey);
	}, [selected.size, clearSelection, runBatchDelete]);

	return {
		selected,
		clearSelection,
		handleSelectRow,
		orderedSelected,
		pathsForAction,
		runBatchDelete,
		cutSelected,
		pasteIntoSelected,
	};
}
