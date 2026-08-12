/**
 * Rail layout controller: owns the resizable panel refs and the
 * imperative collapse / expand transitions, and registers them into the
 * ui-store so plain actions (palette, shortcuts, agent) can drive layout.
 */

import { type RefObject, useEffect, useMemo, useRef } from "react";
import { usePanelRef } from "react-resizable-panels";
import {
	registerLayoutController,
	setRightSidebarOpenState,
	setSidebarCollapsedState,
} from "@/lib/shell/ui-store";
import { toggleNotesSplit } from "@/lib/workspace/actions";
import { getActiveTabId, getTabs } from "@/lib/workspace/store";
import { tabHasNotesSplit, tabNotesEligible } from "@/lib/workspace/tabs";

export const SIDEBAR_DEFAULT_PX = 200;
export const RIGHT_SIDEBAR_DEFAULT_PX = 320;

function prefersReducedMotion(): boolean {
	return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export type ShellLayout = {
	sidebarPanelRef: ReturnType<typeof usePanelRef>;
	rightSidebarPanelRef: ReturnType<typeof usePanelRef>;
	sourcePanelRef: ReturnType<typeof usePanelRef>;
	sidebarAsideRef: RefObject<HTMLElement | null>;
	editorPaneRef: RefObject<HTMLDivElement | null>;
	/** Last expanded rail widths in px (survive collapse / PDF immersive round-trips). */
	leftWidthPxRef: RefObject<number>;
	rightWidthPxRef: RefObject<number>;
	/** Which rail is running a programmatic collapse/expand transition. */
	animatingRailRef: RefObject<"left" | "right" | null>;
	cancelRailAnimation: () => void;
};

/** Keep in sync with --motion-duration-normal in index.css. */
const RAIL_ANIMATION_MS = 200;

export function useShellLayout(): ShellLayout {
	const sidebarPanelRef = usePanelRef();
	const rightSidebarPanelRef = usePanelRef();
	const sourcePanelRef = usePanelRef();
	const sidebarAsideRef = useRef<HTMLElement>(null);
	const editorPaneRef = useRef<HTMLDivElement>(null);
	const leftWidthPxRef = useRef(SIDEBAR_DEFAULT_PX);
	const rightWidthPxRef = useRef(RIGHT_SIDEBAR_DEFAULT_PX);
	const animatingRailRef = useRef<"left" | "right" | null>(null);
	const railAnimTimerRef = useRef(0);

	const controller = useMemo(() => {
		const clearRailAnimating = () => {
			for (const el of document.querySelectorAll("[data-rail-animating]")) {
				el.removeAttribute("data-rail-animating");
			}
		};

		const cancelRailAnimation = () => {
			if (railAnimTimerRef.current) {
				window.clearTimeout(railAnimTimerRef.current);
				railAnimTimerRef.current = 0;
			}
			clearRailAnimating();
			animatingRailRef.current = null;
		};

		/**
		 * The library sizes panels via flex-grow and snaps resize() between
		 * collapsedSize and minSize, so tweening resize() is impossible.
		 * Instead: mark every panel in the group so `flex-grow` transitions
		 * (see index.css), then let collapse()/expand() commit the final layout
		 * — the browser animates all panels in lockstep. A user drag on a
		 * separator cancels the transition first (see App handles).
		 */
		const withRailAnimation = (
			side: "left" | "right",
			panelEl: HTMLElement | null,
			apply: () => void,
		) => {
			if (prefersReducedMotion() || !panelEl) {
				apply();
				return;
			}
			cancelRailAnimation();
			animatingRailRef.current = side;
			const groupEl = panelEl.closest("[data-group]") ?? panelEl.parentElement;
			const targets = groupEl
				? groupEl.querySelectorAll("[data-panel]")
				: [panelEl];
			for (const el of targets) el.setAttribute("data-rail-animating", "");
			apply();
			railAnimTimerRef.current = window.setTimeout(() => {
				railAnimTimerRef.current = 0;
				clearRailAnimating();
				animatingRailRef.current = null;
			}, RAIL_ANIMATION_MS + 40);
		};

		/** Collapse / expand left file-tree panel without remounting. */
		const setLeftCollapsed = (collapsed: boolean) => {
			const panel = sidebarPanelRef.current;
			if (panel) {
				const el = document.getElementById("sidebar");
				if (collapsed) {
					withRailAnimation("left", el, () => {
						try {
							panel.collapse();
						} catch {
							// ignore
						}
					});
				} else {
					const targetPx = leftWidthPxRef.current || SIDEBAR_DEFAULT_PX;
					withRailAnimation("left", el, () => {
						try {
							panel.expand();
							panel.resize(targetPx);
						} catch {
							// ignore
						}
					});
					// expand() fires onResize synchronously and may overwrite the
					// remembered width with the library's default expand size.
					leftWidthPxRef.current = targetPx;
				}
			}
			setSidebarCollapsedState(collapsed);
		};

		/** Collapse / expand right Agent/Backlinks panel (always mounted). */
		const setRightCollapsed = (
			collapsed: boolean,
			_opts?: { focusAgent?: boolean },
		) => {
			const panel = rightSidebarPanelRef.current;
			if (panel) {
				const el = document.getElementById("right-sidebar");
				if (collapsed) {
					withRailAnimation("right", el, () => {
						try {
							panel.collapse();
						} catch {
							// ignore
						}
					});
				} else {
					const targetPx = rightWidthPxRef.current || RIGHT_SIDEBAR_DEFAULT_PX;
					withRailAnimation("right", el, () => {
						try {
							panel.expand();
							panel.resize(targetPx);
						} catch {
							// ignore
						}
					});
					rightWidthPxRef.current = targetPx;
				}
			}
			setRightSidebarOpenState(!collapsed);
		};

		const focusSidebar = () => {
			setLeftCollapsed(false);
			requestAnimationFrame(() => {
				sidebarAsideRef.current?.querySelector<HTMLElement>("button")?.focus();
			});
		};

		const focusEditorPane = () => {
			editorPaneRef.current
				?.querySelector<HTMLElement>("[contenteditable='true']")
				?.focus();
		};

		const focusNotesEditor = () => {
			const tab = getTabs().find((t) => t.id === getActiveTabId());
			if (tab && tabNotesEligible(tab) && !tabHasNotesSplit(getTabs(), tab)) {
				toggleNotesSplit();
			}
			requestAnimationFrame(() => {
				requestAnimationFrame(() => {
					// Prefer a NOTES secondary-pane editor when present.
					const root = editorPaneRef.current;
					if (!root) return;
					const editables = root.querySelectorAll<HTMLElement>(
						"[contenteditable='true']",
					);
					const target = editables[editables.length - 1] ?? editables[0];
					target?.focus();
				});
			});
		};

		return {
			setLeftCollapsed,
			setRightCollapsed,
			focusSidebar,
			focusEditorPane,
			focusNotesEditor,
			cancelRailAnimation,
		};
	}, [sidebarPanelRef, rightSidebarPanelRef]);

	useEffect(() => {
		registerLayoutController(controller);
		return () => {
			registerLayoutController(null);
			controller.cancelRailAnimation();
		};
	}, [controller]);

	return {
		sidebarPanelRef,
		rightSidebarPanelRef,
		sourcePanelRef,
		sidebarAsideRef,
		editorPaneRef,
		leftWidthPxRef,
		rightWidthPxRef,
		animatingRailRef,
		cancelRailAnimation: controller.cancelRailAnimation,
	};
}
