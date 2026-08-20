/**
 * Layout-region hover for the PDF viewer, and the two cards it coordinates.
 *
 * Clicking a figure, table or algorithm opens a region-crop draft card (wired
 * in the page layers, not here); dwelling on a formula opens the
 * `{paper}/Annotation.md` glossary card. The two cards are **mutually
 * exclusive**, which is why this hook owns both `visualDraftEditor` and
 * `formulaAnnotationPreview` even though the draft card's *content* belongs to
 * {@link usePdfVisualMarks}: the setters stay private and
 * `openVisualDraftEditor` / `closeVisualDraftEditor` /
 * `closeFormulaAnnotationPreview` are the only transitions, so neither state
 * can move without the other's guard.
 */

import {
	type RefObject,
	useCallback,
	useEffect,
	useRef,
	useState,
} from "react";
import { pageElByIndex } from "@/components/viewer/pdf/coords";
import type {
	FormulaAnnotationPreviewState,
	ScreenPoint,
	SelectionMenuState,
	VisualDraftEditorState,
} from "@/components/viewer/pdf/types";
import { listenSafe } from "@/lib/core/tauri-events";
import type { PdfAskNormalizedRect } from "@/lib/pdf/ask/types";
import {
	type EquationSymbol,
	equationAnnotationPath,
	loadEquationAnnotation,
} from "@/lib/pdf/equation-annotation";
import {
	isFormulaLayoutKind,
	isLayoutHoverSuppressedByScroll,
	LAYOUT_FORMULA_HOVER_DWELL_MS,
	LAYOUT_FORMULA_HOVER_HIDE_MS,
	type PdfLayoutRegion,
	setFocusedLayoutRegion,
} from "@/lib/pdf/layout";
import {
	VAULT_FILE_CHANGED_EVENT,
	type VaultFileChangedPayload,
} from "@/lib/vault/fs-watch";
import { normalizePathKey } from "@/lib/vault/path";

export type UsePdfLayoutHoverOptions = {
	docId: string;
	/** Paper folder: `Annotation.md` glossary root (null for loose PDFs). */
	paperAbsPath: string | null;
	hostRef: RefObject<HTMLDivElement | null>;
	/** Current zoom: the open formula legend re-anchors when it changes. */
	zoomLevel: number;
	/** Text-selection cluster: an open menu blocks layout hover. */
	selectionMenuRef: RefObject<SelectionMenuState | null>;
	/**
	 * Visual-mark cluster mirrors (parent-owned): region framing or an in-flight
	 * crop blocks layout hover.
	 */
	regionSelectingRef: RefObject<boolean>;
	visualCropPendingRef: RefObject<boolean>;
};

export type PdfLayoutHover = {
	/** Parsed rows from `{paper}/Annotation.md` (empty when missing). */
	equationSymbols: EquationSymbol[];
	/** Region-crop draft card. Mutually exclusive with the formula legend. */
	visualDraftEditor: VisualDraftEditorState | null;
	/** Formula glossary card. Mutually exclusive with the draft card. */
	formulaAnnotationPreview: FormulaAnnotationPreviewState | null;
	/** Sole entry point for the draft card; closes the formula legend first. */
	openVisualDraftEditor: (draft: VisualDraftEditorState) => void;
	closeVisualDraftEditor: () => void;
	closeFormulaAnnotationPreview: () => void;
	/** Screen anchor beside a page-normalized region (hover card placement). */
	screenPointForRegion: (
		pageIndex0: number,
		region: PdfAskNormalizedRect,
	) => ScreenPoint;
	/** Pointer entered a layout hit target → start the formula legend dwell. */
	scheduleLayoutHoverOpen: (region: PdfLayoutRegion) => void;
	handleLayoutHoverLeave: (regionId: string) => void;
	markFormulaHoverEnter: () => void;
	scheduleFormulaHide: () => void;
	/** Re-anchor the open formula legend after the page moved under it. */
	rePlaceFormulaAnnotationOnScroll: () => void;
};

export function usePdfLayoutHover({
	docId,
	paperAbsPath,
	hostRef,
	zoomLevel,
	selectionMenuRef,
	regionSelectingRef,
	visualCropPendingRef,
}: UsePdfLayoutHoverOptions): PdfLayoutHover {
	const [visualDraftEditor, setVisualDraftEditor] =
		useState<VisualDraftEditorState | null>(null);
	/** Formula hover → Annotation.md symbol glossary (when present). */
	const [formulaAnnotationPreview, setFormulaAnnotationPreview] =
		useState<FormulaAnnotationPreviewState | null>(null);
	/** Parsed rows from `{paper}/Annotation.md` (empty when missing). */
	const [equationSymbols, setEquationSymbols] = useState<EquationSymbol[]>([]);
	const visualDraftEditorRef = useRef(visualDraftEditor);
	visualDraftEditorRef.current = visualDraftEditor;
	const formulaAnnotationPreviewRef = useRef(formulaAnnotationPreview);
	formulaAnnotationPreviewRef.current = formulaAnnotationPreview;
	const equationSymbolsRef = useRef(equationSymbols);
	equationSymbolsRef.current = equationSymbols;

	/** Formula legend dwell (tooltip-like). */
	const formulaHoverTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	const formulaHoverRegionIdRef = useRef<string | null>(null);
	/** Formula legend auto-hide after leave region / card. */
	const formulaHideTimerRef = useRef<ReturnType<typeof setTimeout> | null>(
		null,
	);
	/** True while pointer is over the formula hit region or legend card. */
	const formulaHoverSurfaceRef = useRef(false);
	/** Last wheel / scroll inside the PDF viewport; suppresses accidental hover. */
	const lastLayoutScrollAtRef = useRef(0);

	const nowMs = useCallback(() => performance.now(), []);

	const layoutHoverSuppressedByScroll = useCallback(
		() =>
			isLayoutHoverSuppressedByScroll(lastLayoutScrollAtRef.current, nowMs()),
		[nowMs],
	);

	// Load `{paper}/Annotation.md` symbol glossary for formula hover cards.
	useEffect(() => {
		let cancelled = false;
		if (!paperAbsPath) {
			setEquationSymbols([]);
			setFormulaAnnotationPreview(null);
			return;
		}
		void loadEquationAnnotation(paperAbsPath).then((symbols) => {
			if (cancelled) return;
			setEquationSymbols(symbols);
		});
		return () => {
			cancelled = true;
		};
	}, [paperAbsPath]);

	// Reload Annotation.md when the Agent / editor rewrites it on disk.
	useEffect(() => {
		if (!paperAbsPath) return;
		const annotationPath = equationAnnotationPath(paperAbsPath);
		const annotationKey = normalizePathKey(annotationPath);
		return listenSafe<VaultFileChangedPayload>(
			VAULT_FILE_CHANGED_EVENT,
			(payload) => {
				const paths = [...payload.paths];
				if (payload.rename) {
					paths.push(payload.rename.from, payload.rename.to);
				}
				const hit = paths.some((p) => normalizePathKey(p) === annotationKey);
				if (!hit) return;
				void loadEquationAnnotation(paperAbsPath).then((symbols) => {
					setEquationSymbols(symbols);
					// Drop open card if the glossary disappeared.
					if (symbols.length === 0) {
						setFormulaAnnotationPreview(null);
					} else {
						setFormulaAnnotationPreview((prev) =>
							prev ? { ...prev, symbols } : prev,
						);
					}
				});
			},
		);
	}, [paperAbsPath]);

	const cancelFormulaHover = useCallback((regionId?: string) => {
		if (
			regionId != null &&
			formulaHoverRegionIdRef.current != null &&
			formulaHoverRegionIdRef.current !== regionId
		) {
			return;
		}
		if (formulaHoverTimerRef.current) {
			clearTimeout(formulaHoverTimerRef.current);
			formulaHoverTimerRef.current = null;
		}
		if (regionId == null || formulaHoverRegionIdRef.current === regionId) {
			formulaHoverRegionIdRef.current = null;
		}
	}, []);

	const cancelFormulaHide = useCallback(() => {
		if (!formulaHideTimerRef.current) return;
		clearTimeout(formulaHideTimerRef.current);
		formulaHideTimerRef.current = null;
	}, []);

	const closeVisualDraftEditor = useCallback(() => {
		setVisualDraftEditor(null);
	}, []);

	const closeFormulaAnnotationPreview = useCallback(() => {
		cancelFormulaHover();
		cancelFormulaHide();
		formulaHoverSurfaceRef.current = false;
		const had = formulaAnnotationPreviewRef.current != null;
		setFormulaAnnotationPreview(null);
		if (had && !visualDraftEditorRef.current) {
			setFocusedLayoutRegion(docId, null);
		}
	}, [cancelFormulaHide, cancelFormulaHover, docId]);

	/** Keep formula legend open while pointer is on the hit region or card. */
	const markFormulaHoverEnter = useCallback(() => {
		formulaHoverSurfaceRef.current = true;
		cancelFormulaHide();
	}, [cancelFormulaHide]);

	/**
	 * Leave formula hit / legend card → close after a short grace so the
	 * pointer can cross the gap into the floating card.
	 */
	const scheduleFormulaHide = useCallback(() => {
		if (!formulaAnnotationPreviewRef.current) return;
		formulaHoverSurfaceRef.current = false;
		cancelFormulaHide();
		formulaHideTimerRef.current = setTimeout(() => {
			formulaHideTimerRef.current = null;
			if (formulaHoverSurfaceRef.current) return;
			if (!formulaAnnotationPreviewRef.current) return;
			closeFormulaAnnotationPreview();
		}, LAYOUT_FORMULA_HOVER_HIDE_MS);
	}, [cancelFormulaHide, closeFormulaAnnotationPreview]);

	useEffect(() => {
		const host = hostRef.current;
		if (!host) return;
		const markScroll = () => {
			lastLayoutScrollAtRef.current = nowMs();
		};
		host.addEventListener("wheel", markScroll, {
			capture: true,
			passive: true,
		});
		host.addEventListener("scroll", markScroll, {
			capture: true,
			passive: true,
		});
		return () => {
			host.removeEventListener("wheel", markScroll, true);
			host.removeEventListener("scroll", markScroll, true);
		};
	}, [hostRef, nowMs]);

	/** Screen point near a layout bbox (right edge) for hover cards. */
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

	/** Open / switch the formula legend card for a layout region. */
	const openFormulaLegend = useCallback(
		(region: PdfLayoutRegion) => {
			const symbols = equationSymbolsRef.current;
			if (symbols.length === 0) return;
			// Pointer is still on the formula hit when we open; keep surface live
			// so unmount/remount of overlays does not immediately hide.
			formulaHoverSurfaceRef.current = true;
			cancelFormulaHide();
			cancelFormulaHover();
			setFocusedLayoutRegion(docId, region.id);
			setFormulaAnnotationPreview({
				screen: screenPointForRegion(region.pageIndex, region.bbox),
				regionId: region.id,
				page: region.pageIndex + 1,
				region: region.bbox,
				symbols,
			});
		},
		[cancelFormulaHide, cancelFormulaHover, docId, screenPointForRegion],
	);

	/** Re-anchor the open formula legend after scroll / zoom. */
	const rePlaceFormulaAnnotationOnScroll = useCallback(() => {
		const prev = formulaAnnotationPreviewRef.current;
		if (!prev) return;
		const screen = screenPointForRegion(prev.page - 1, prev.region);
		setFormulaAnnotationPreview((current) => {
			if (!current || current.regionId !== prev.regionId) return current;
			if (current.screen.x === screen.x && current.screen.y === screen.y) {
				return current;
			}
			return { ...current, screen };
		});
	}, [screenPointForRegion]);

	/**
	 * Open the region-crop draft card. Sole entry point for `visualDraftEditor`,
	 * so the「draft ⇄ formula legend are mutually exclusive」invariant lives with
	 * both states instead of in every caller.
	 */
	const openVisualDraftEditor = useCallback(
		(draft: VisualDraftEditorState) => {
			// Visual draft and formula legend are mutually exclusive.
			closeFormulaAnnotationPreview();
			setVisualDraftEditor(draft);
		},
		[closeFormulaAnnotationPreview],
	);

	/**
	 * True while another interaction owns the page: region framing, an in-flight
	 * crop, an open visual draft, or the selection menu. Layout hover must not
	 * open on top of any of them.
	 */
	const layoutHoverBlocked = useCallback(
		() =>
			Boolean(
				regionSelectingRef.current ||
					visualCropPendingRef.current ||
					visualDraftEditorRef.current ||
					selectionMenuRef.current,
			),
		[selectionMenuRef, regionSelectingRef, visualCropPendingRef],
	);

	/**
	 * After dwelling on a formula region with Annotation.md symbols →「公式解析」
	 * glossary card. Figures / tables / algorithms ignore hover (click opens the
	 * draft card; the hit target shows the「单击进行批注」hint).
	 */
	const scheduleLayoutHoverOpen = useCallback(
		(region: PdfLayoutRegion) => {
			if (layoutHoverBlocked()) return;
			if (layoutHoverSuppressedByScroll()) return;
			const symbols = equationSymbolsRef.current;
			if (!isFormulaLayoutKind(region.kind) || symbols.length === 0) return;
			// Don't stack a formula legend while a visual draft is open
			// (layoutHoverBlocked already covers it; keep the cards exclusive
			// in both directions).
			if (visualDraftEditorRef.current) return;

			// Already showing this formula: cancel pending hide, stay open.
			if (formulaAnnotationPreviewRef.current?.regionId === region.id) {
				markFormulaHoverEnter();
				return;
			}
			// Switching formulas: open the new one after a short dwell (or
			// immediately if a legend is already open — seamless switch).
			if (
				formulaHoverRegionIdRef.current === region.id &&
				formulaHoverTimerRef.current
			) {
				return;
			}
			cancelFormulaHover();
			// Switching while another legend is open: no extra dwell.
			if (formulaAnnotationPreviewRef.current) {
				openFormulaLegend(region);
				return;
			}
			formulaHoverRegionIdRef.current = region.id;
			formulaHoverTimerRef.current = setTimeout(() => {
				formulaHoverTimerRef.current = null;
				if (formulaHoverRegionIdRef.current !== region.id) return;
				if (layoutHoverBlocked()) return;
				if (layoutHoverSuppressedByScroll()) return;
				openFormulaLegend(region);
			}, LAYOUT_FORMULA_HOVER_DWELL_MS);
		},
		[
			cancelFormulaHover,
			layoutHoverBlocked,
			layoutHoverSuppressedByScroll,
			markFormulaHoverEnter,
			openFormulaLegend,
		],
	);

	const handleLayoutHoverLeave = useCallback(
		(regionId: string) => {
			// Formula dwell / open legend for this region.
			if (formulaHoverRegionIdRef.current === regionId) {
				cancelFormulaHover(regionId);
			}
			if (formulaAnnotationPreviewRef.current?.regionId === regionId) {
				scheduleFormulaHide();
			}
		},
		[cancelFormulaHover, scheduleFormulaHide],
	);

	useEffect(() => {
		// Drop in-flight hover when switching PDF documents or unmounting
		// (closeFormulaAnnotationPreview identity tracks docId).
		closeFormulaAnnotationPreview();
		return () => {
			closeFormulaAnnotationPreview();
		};
	}, [closeFormulaAnnotationPreview]);

	// Escape closes the formula legend (same expectation as other floaters).
	useEffect(() => {
		if (!formulaAnnotationPreview) return;
		const onKeyDown = (e: KeyboardEvent) => {
			if (e.key !== "Escape") return;
			e.preventDefault();
			closeFormulaAnnotationPreview();
		};
		window.addEventListener("keydown", onKeyDown);
		return () => window.removeEventListener("keydown", onKeyDown);
	}, [formulaAnnotationPreview, closeFormulaAnnotationPreview]);

	// Keep formula legend glued to its bbox across zoom (scroll uses ActiveCardScrollSync).
	// biome-ignore lint/correctness/useExhaustiveDependencies: zoomLevel re-places intentionally
	useEffect(() => {
		if (!formulaAnnotationPreview) return;
		rePlaceFormulaAnnotationOnScroll();
	}, [
		formulaAnnotationPreview?.regionId,
		zoomLevel,
		rePlaceFormulaAnnotationOnScroll,
	]);

	return {
		equationSymbols,
		visualDraftEditor,
		formulaAnnotationPreview,
		openVisualDraftEditor,
		closeVisualDraftEditor,
		closeFormulaAnnotationPreview,
		screenPointForRegion,
		scheduleLayoutHoverOpen,
		handleLayoutHoverLeave,
		markFormulaHoverEnter,
		scheduleFormulaHide,
		rePlaceFormulaAnnotationOnScroll,
	};
}
