/**
 * Workspace (dockview panels) state — zustand vanilla.
 * Tabs are the flat panel list; layout is owned by dockview (`toJSON()` blob).
 * Plain modules read the latest snapshot via `getState()`, which replaces the
 * old tabsRef/activeTabIdRef mirrors in App.
 */

import { createStore } from "zustand/vanilla";
import { isTauri } from "@/lib/core/tauri";
import {
	createPlaceholderTab,
	type DocTab,
	loadPersistedTabs,
	patchTab,
	reseedMarkdownTab,
	reseedNotesTab,
} from "@/lib/workspace/tabs";

type WorkspaceStore = {
	/** Open document panels (flat list; layout owned by global dockview). */
	tabs: DocTab[];
	activeTabId: string | null;
	/** Global dockview grid snapshot (panel ids = tab ids). */
	dockLayout: unknown | null;
	/** Most-recently-viewed PDF tab ids kept mounted (see PDF_TAB_MOUNT_LRU). */
	pdfLru: string[];
	/** Most-recently-viewed Markdown editor tab ids kept mounted (see EDITOR_TAB_MOUNT_LRU). */
	editorLru: string[];
};

export const workspaceStore = createStore<WorkspaceStore>(() => ({
	tabs: [],
	activeTabId: null,
	dockLayout: null,
	pdfLru: [],
	editorLru: [],
}));

let initialized = false;

/**
 * Seed open panels + layout from localStorage before first paint so
 * DockWorkspace onReady can fromJSON before any membership sync.
 */
export function initWorkspaceStore(): void {
	if (initialized) return;
	initialized = true;
	if (!isTauri()) return;
	const persisted = loadPersistedTabs();
	if (!persisted?.tabs.length) return;
	workspaceStore.setState({
		tabs: persisted.tabs.map((pt) =>
			createPlaceholderTab(pt.path, pt.mode, pt.id),
		),
		activeTabId: persisted.activeId ?? null,
		dockLayout: persisted.layout ?? null,
	});
}

export function getTabs(): DocTab[] {
	return workspaceStore.getState().tabs;
}

export function getActiveTabId(): string | null {
	return workspaceStore.getState().activeTabId;
}

export function getActiveTab(): DocTab | null {
	const { tabs, activeTabId } = workspaceStore.getState();
	return tabs.find((t) => t.id === activeTabId) ?? null;
}

export function setTabs(
	next: DocTab[] | ((previous: DocTab[]) => DocTab[]),
): void {
	if (typeof next === "function") {
		workspaceStore.setState((s) => ({ tabs: next(s.tabs) }));
		return;
	}
	workspaceStore.setState({ tabs: next });
}

export function setActiveTabId(id: string | null): void {
	workspaceStore.setState({ activeTabId: id });
}

export function setDockLayout(layout: unknown | null): void {
	workspaceStore.setState({ dockLayout: layout });
}

export function setPdfLru(
	next: string[] | ((previous: string[]) => string[]),
): void {
	if (typeof next === "function") {
		workspaceStore.setState((s) => ({ pdfLru: next(s.pdfLru) }));
		return;
	}
	workspaceStore.setState({ pdfLru: next });
}

export function setEditorLru(
	next: string[] | ((previous: string[]) => string[]),
): void {
	if (typeof next === "function") {
		workspaceStore.setState((s) => ({ editorLru: next(s.editorLru) }));
		return;
	}
	workspaceStore.setState({ editorLru: next });
}

/** Merge a patch into the panel with the given id. */
export function updateTab(id: string, patch: Partial<DocTab>): void {
	setTabs((prev) => patchTab(prev, id, patch));
}

export function toggleTabHtmlMode(id: string): void {
	setTabs((prev) =>
		prev.map((tab) => {
			if (
				tab.id !== id ||
				tab.paperMeta?.type === "html" ||
				!tab.htmlUrl ||
				(tab.mode !== "pdf" && tab.mode !== "html")
			) {
				return tab;
			}
			return { ...tab, mode: tab.mode === "pdf" ? "html" : "pdf" };
		}),
	);
}

/** Reseed an open paper tab's NOTES after the reader / download writes it. */
export function refreshTabNotes(paperDir: string, content: string): void {
	setTabs((prev) => reseedNotesTab(prev, paperDir, content));
}

/** Reseed an open plain-Markdown tab after an external/Agent write. */
export function refreshTabMarkdown(absPath: string, content: string): void {
	setTabs((prev) => reseedMarkdownTab(prev, absPath, content));
}
