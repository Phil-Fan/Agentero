/**
 * Pure derivations over the vault tree: path indexes, display order, the
 * highlighted row, and the flattened row list consumed by the virtualizer.
 */
import { useCallback, useMemo } from "react";
import {
	isPaperDirectory,
	type PaperMetadata,
	type PaperTreeLabelMode,
	type PaperTreeSortMode,
	sortFileTreeNodes,
} from "@/lib/paper";
import type { FileNode } from "@/lib/vault";
import { toVaultRelative } from "@/lib/wiki";
import { isVirtualTreePath, pathKey } from "../tree-helpers";
import type { FlatRow, TreeCreateDraft } from "../types";

export type TreeIndex = {
	byPath: ReadonlyMap<string, FileNode>;
	/** Case-insensitive path → node (selection resolve / ancestor expand). */
	byPathKey: ReadonlyMap<string, FileNode>;
	relPathForNode: (absPath: string) => string;
	/** Siblings ordered per Settings → paperTreeSortMode. */
	displayNodes: FileNode[];
	/**
	 * Row to highlight / scroll to:
	 * - virtual Library / Trash as-is;
	 * - any path under a paper folder → that paper (papers are tree leaves);
	 * - otherwise the path itself if present, else nearest existing ancestor.
	 */
	treeSelectedPath: string | undefined;
};

export function useTreeIndex({
	nodes,
	vaultPath,
	selectedPath,
	paperMetaByRelPath,
	paperTreeLabelMode,
	paperTreeSortMode,
}: {
	nodes: FileNode[];
	vaultPath: string | null;
	selectedPath: string | null;
	paperMetaByRelPath?: ReadonlyMap<string, PaperMetadata>;
	paperTreeLabelMode: PaperTreeLabelMode;
	paperTreeSortMode: PaperTreeSortMode;
}): TreeIndex {
	const byPath = useMemo(() => {
		const map = new Map<string, FileNode>();
		const walk = (list: FileNode[]) => {
			for (const n of list) {
				map.set(n.path, n);
				if (n.children) walk(n.children);
			}
		};
		walk(nodes);
		return map;
	}, [nodes]);

	const byPathKey = useMemo(() => {
		const map = new Map<string, FileNode>();
		const walk = (list: FileNode[]) => {
			for (const n of list) {
				map.set(pathKey(n.path), n);
				if (n.children) walk(n.children);
			}
		};
		walk(nodes);
		return map;
	}, [nodes]);

	const relPathForNode = useCallback(
		(absPath: string): string => toVaultRelative(vaultPath, absPath),
		[vaultPath],
	);

	const displayNodes = useMemo(
		() =>
			sortFileTreeNodes(
				nodes,
				paperTreeSortMode,
				paperMetaByRelPath,
				relPathForNode,
				paperTreeLabelMode,
			),
		[
			nodes,
			paperTreeSortMode,
			paperMetaByRelPath,
			relPathForNode,
			paperTreeLabelMode,
		],
	);

	const treeSelectedPath = useMemo(() => {
		if (!selectedPath) return undefined;
		if (isVirtualTreePath(selectedPath)) return selectedPath;

		// Prefer paper folder: children of papers are not listed in the tree.
		let cursor = selectedPath.replace(/\\/g, "/").replace(/\/+$/, "");
		while (cursor) {
			const node = byPathKey.get(pathKey(cursor));
			if (
				node &&
				node.kind === "directory" &&
				isPaperDirectory(node.path, node.children)
			) {
				return node.path;
			}
			const idx = cursor.lastIndexOf("/");
			if (idx <= 0) break;
			cursor = cursor.slice(0, idx);
		}

		const exact = byPathKey.get(pathKey(selectedPath));
		if (exact) return exact.path;

		// Deleted / not-yet-in-tree: nearest existing ancestor.
		cursor = selectedPath.replace(/\\/g, "/").replace(/\/+$/, "");
		while (true) {
			const idx = cursor.lastIndexOf("/");
			if (idx <= 0) break;
			cursor = cursor.slice(0, idx);
			const node = byPathKey.get(pathKey(cursor));
			if (node) return node.path;
		}
		return selectedPath;
	}, [selectedPath, byPathKey]);

	return {
		byPath,
		byPathKey,
		relPathForNode,
		displayNodes,
		treeSelectedPath,
	};
}

export type TreeRows = {
	/** Visible, selectable rows in display order (paper folders are leaves). */
	selectableOrder: string[];
	/** Flattened rows in display order (respects expand state + inline drafts). */
	flatRows: FlatRow[];
};

export function useTreeRows({
	displayNodes,
	expanded,
	createDraft,
	vaultPath,
}: {
	displayNodes: FileNode[];
	expanded: ReadonlySet<string>;
	createDraft: TreeCreateDraft | null;
	vaultPath: string | null;
}): TreeRows {
	const selectableOrder = useMemo(() => {
		const out: string[] = [];
		const walk = (list: FileNode[]) => {
			for (const n of list) {
				out.push(n.path);
				if (
					n.kind === "directory" &&
					!isPaperDirectory(n.path, n.children) &&
					expanded.has(n.path) &&
					n.children?.length
				) {
					walk(n.children);
				}
			}
		};
		walk(displayNodes);
		return out;
	}, [displayNodes, expanded]);

	const flatRows = useMemo<FlatRow[]>(() => {
		// Virtual Library + Recycle Bin sit at the top (Library, then trash).
		const out: FlatRow[] = [
			{ key: "__library__", kind: "library" },
			{ key: "__trash__", kind: "trash" },
		];
		const draftAt = (parent: string) =>
			Boolean(
				createDraft && pathKey(createDraft.parentPath) === pathKey(parent),
			);
		if (vaultPath && draftAt(vaultPath)) {
			out.push({ key: "__create_root__", kind: "create", depth: 0 });
		}
		const walk = (list: FileNode[], depth: number) => {
			for (const n of list) {
				const paper =
					n.kind === "directory" && isPaperDirectory(n.path, n.children);
				out.push({
					key: n.id,
					kind: "node",
					depth,
					node: n,
					paperLeaf: paper,
				});
				if (n.kind === "directory" && draftAt(n.path)) {
					out.push({
						key: `create-${n.path}`,
						kind: "create",
						depth: depth + 1,
					});
				}
				if (
					n.kind === "directory" &&
					!paper &&
					expanded.has(n.path) &&
					n.children?.length
				) {
					walk(n.children, depth + 1);
				}
			}
		};
		walk(displayNodes, 0);
		return out;
	}, [displayNodes, expanded, createDraft, vaultPath]);

	return { selectableOrder, flatRows };
}
