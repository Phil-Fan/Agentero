/**
 * Papers library state (zustand vanilla): catalog rows, search query, folder
 * scope, and import/export busy flags. Query keystrokes now only re-render
 * library subscribers instead of the whole App.
 */

import { createStore } from "zustand/vanilla";
import { isTauri } from "@/lib/core/tauri";
import type { PaperMetadata } from "@/lib/paper";
import { listPapers } from "@/lib/paper/api";
import type { LocalPdfImportEntry } from "@/lib/paper/lookup";
import { getVaultPath } from "@/lib/vault/store";

export type LibraryIoBusy = "import" | "export" | "import-pdf" | null;

export type { LocalPdfImportEntry };

type LibraryStore = {
	papers: PaperMetadata[];
	loading: boolean;
	/** Title search query for the papers library view. */
	query: string;
	/**
	 * Vault-relative folder filter for the single Library tab. Null = full
	 * library. Set by clicking org folders in the tree — no new tabs.
	 */
	scopePath: string | null;
	rescanning: boolean;
	ioBusy: LibraryIoBusy;
	/** Paper being edited in the Edit Metadata dialog (null = closed). */
	editMetaDraft: PaperMetadata | null;
	/** Bump to force RecycleBinView reload after Empty Recycle Bin. */
	trashReloadSignal: number;
	/** Catalog rows by vault-relative path (for Zap / is_read). */
	paperMetaByRelPath: Map<string, PaperMetadata>;
};

function indexByRelPath(papers: PaperMetadata[]): Map<string, PaperMetadata> {
	const map = new Map<string, PaperMetadata>();
	for (const p of papers) {
		if (!p.path) continue;
		map.set(p.path.replace(/\\/g, "/").replace(/^\/+|\/+$/g, ""), p);
	}
	return map;
}

export const libraryStore = createStore<LibraryStore>(() => ({
	papers: [],
	loading: false,
	query: "",
	scopePath: null,
	rescanning: false,
	ioBusy: null,
	editMetaDraft: null,
	trashReloadSignal: 0,
	paperMetaByRelPath: new Map(),
}));

function tagsFingerprint(tags: PaperMetadata["tags"]): string {
	if (!tags?.length) return "";
	return tags
		.map((t) => (typeof t === "string" ? t : `${t.name}:${t.color ?? ""}`))
		.join("|");
}

/**
 * Content equality for catalog refreshes: watcher-driven reloads usually
 * return identical rows, and replacing the array anyway would re-render the
 * whole library and retrigger heatmap loads for every paper.
 */
function samePapers(a: PaperMetadata[], b: PaperMetadata[]): boolean {
	if (a === b) return true;
	if (a.length !== b.length) return false;
	for (let i = 0; i < a.length; i++) {
		const x = a[i];
		const y = b[i];
		if (x === y) continue;
		if (
			x.path !== y.path ||
			x.updated_at !== y.updated_at ||
			x.is_read !== y.is_read ||
			x.title !== y.title ||
			tagsFingerprint(x.tags) !== tagsFingerprint(y.tags)
		) {
			return false;
		}
	}
	return true;
}

export function setLibraryPapers(
	next: PaperMetadata[] | ((previous: PaperMetadata[]) => PaperMetadata[]),
): void {
	const papers =
		typeof next === "function" ? next(libraryStore.getState().papers) : next;
	const prev = libraryStore.getState();
	if (papers === prev.papers || samePapers(prev.papers, papers)) return;
	libraryStore.setState({ papers, paperMetaByRelPath: indexByRelPath(papers) });
}

export function setLibraryQuery(query: string): void {
	libraryStore.setState({ query });
}

export function setLibraryScopePath(
	next: string | null | ((previous: string | null) => string | null),
): void {
	if (typeof next === "function") {
		libraryStore.setState((s) => ({ scopePath: next(s.scopePath) }));
		return;
	}
	libraryStore.setState({ scopePath: next });
}

export function setLibraryRescanning(rescanning: boolean): void {
	libraryStore.setState({ rescanning });
}

export function setLibraryIoBusy(ioBusy: LibraryIoBusy): void {
	libraryStore.setState({ ioBusy });
}

export function setEditMetaDraft(draft: PaperMetadata | null): void {
	libraryStore.setState({ editMetaDraft: draft });
}

export function bumpTrashReloadSignal(): void {
	libraryStore.setState((s) => ({
		trashReloadSignal: s.trashReloadSignal + 1,
	}));
}

export async function refreshLibrary(): Promise<void> {
	const vaultPath = getVaultPath();
	if (!vaultPath || !isTauri()) {
		setLibraryPapers([]);
		return;
	}
	libraryStore.setState({ loading: true });
	try {
		setLibraryPapers(await listPapers(vaultPath));
	} catch {
		setLibraryPapers([]);
	} finally {
		libraryStore.setState({ loading: false });
	}
}

let libraryRefreshTimer: ReturnType<typeof setTimeout> | null = null;

/**
 * Quiet, debounced catalog reload for external tools (CLI, sync clients).
 * Avoids loading-state flicker while still updating tree labels and table rows.
 */
export function scheduleLibraryRefresh(): void {
	if (libraryRefreshTimer) clearTimeout(libraryRefreshTimer);
	libraryRefreshTimer = setTimeout(() => {
		libraryRefreshTimer = null;
		const vaultPath = getVaultPath();
		if (!vaultPath || !isTauri()) {
			setLibraryPapers([]);
			return;
		}
		void listPapers(vaultPath)
			.then((papers) => {
				if (getVaultPath() === vaultPath) setLibraryPapers(papers);
			})
			.catch(() => {
				// Best-effort background refresh; explicit Library opens still report loading.
			});
	}, 500);
}
