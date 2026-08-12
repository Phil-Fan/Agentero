/**
 * Per-page layer stack rendered by EmbedPDF's `<Scroller renderPage>`: raster /
 * tiling / search / selection / annotation layers plus every Agentero overlay
 * (citation hits, layout boxes, bulk-translate text, mark source frames, gutter
 * pins).
 *
 * Memoized because the scroller re-renders every mounted page whenever its
 * layout changes; without a bail-out a single scroll frame rebuilds ten page
 * subtrees. Props are grouped into bundles the parent memoizes, so the shallow
 * comparison stays maintainable — a flat prop list would make it far too easy
 * to silently break memoization.
 */

import type { PdfLinkAnnoObject } from "@embedpdf/models";
import { AnnotationLayer } from "@embedpdf/plugin-annotation/react";
import { PagePointerProvider } from "@embedpdf/plugin-interaction-manager/react";
import { LayoutAnalysisLayer } from "@embedpdf/plugin-layout-analysis/react";
import { RenderLayer } from "@embedpdf/plugin-render/react";
import { SearchLayer } from "@embedpdf/plugin-search/react";
import { SelectionLayer } from "@embedpdf/plugin-selection/react";
import { TilingLayer } from "@embedpdf/plugin-tiling/react";
import { EyeOff, Languages, Loader2 } from "lucide-react";
import { memo, type RefObject, useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import {
	EMPTY_CITATION_LINKS,
	EMPTY_PINS,
	PAGE_LAYER_STYLE,
	PDF_BASE_LAYER_SCALE_CAP,
	pdfRasterDpr,
} from "@/components/viewer/pdf/constants";
import { EMBED_PAGE_ATTR } from "@/components/viewer/pdf/coords";
import { CitationLinkLayer } from "@/components/viewer/pdf/layers/citation-links";
import { HighlightAnnotationMenu } from "@/components/viewer/pdf/layers/highlight-annotation-menu";
import { LayoutTranslateOverlay } from "@/components/viewer/pdf/layers/layout-translate-overlay";
import { PdfRegionSelectLayer } from "@/components/viewer/pdf/layers/region-select-layer";
import { SelectionGutter } from "@/components/viewer/pdf/layers/selection-gutter";
import { cn } from "@/lib/core/utils";
import type { PdfVisualSessionTrace } from "@/lib/pdf/agent-trace";
import type { PdfAskNormalizedRect } from "@/lib/pdf/ask/types";
import type { HighlightColor } from "@/lib/pdf/highlight/palette";
import {
	createLayoutRegionClickGuard,
	isFormulaLayoutKind,
	type LayoutTranslateItem,
	layoutKindBorder,
	layoutKindFill,
	layoutKindHex,
	layoutKindI18nKey,
	type PdfLayoutRegion,
} from "@/lib/pdf/layout";
import { PDF_PAGE_RASTER_DARK_CLASS } from "@/lib/pdf/page-theme";
import type { SelectionPin } from "@/lib/pdf/selection";

/** A mark region pinned to a page (visual draft frame / formula legend frame). */
type PageRegion = { page: number; region: PdfAskNormalizedRect } | null;

/**
 * Anchor geometry of an open ask / translate card. Anchor-only: it keeps its
 * identity while the card body streams, so the page layers skip re-rendering
 * per streamed chunk.
 */
export type PdfActiveCardAnchor = {
	id: string;
	/** 1-based page number */
	page: number;
	rects: PdfAskNormalizedRect[];
};

/** Marks and mark-derived overlays. Whole-document work, bucketed by page. */
export type PdfPageMarksSlice = {
	activeAskAnchor: PdfActiveCardAnchor | null;
	activeTranslateAnchor: PdfActiveCardAnchor | null;
	activeVisualTrace: PdfVisualSessionTrace | null;
	visualDraftRegion: PageRegion;
	formulaAnnotationRegion: PageRegion;
	focusedLayoutRegion: PdfLayoutRegion | null;
	pinsByPage: ReadonlyMap<number, SelectionPin[]>;
	citationLinks: ReadonlyMap<number, PdfLinkAnnoObject[]>;
	activeCardId: string | null;
};

/** Layout-analysis derived overlays (hover targets, debug boxes, translations). */
export type PdfPageLayoutSlice = {
	hoverableRegionsByPage: ReadonlyMap<number, PdfLayoutRegion[]>;
	rawRegionsByPage: ReadonlyMap<number, PdfLayoutRegion[]>;
	layoutOverlayVisible: boolean;
	layoutTranslateItemsByPage: ReadonlyMap<
		number,
		readonly LayoutTranslateItem[]
	>;
	layoutTranslatePageStateByPage: ReadonlyMap<
		number,
		{ active: boolean; running: boolean }
	>;
	equationSymbolCount: number;
	/** Layout-hover drafts auto-hide, so their frame needs a hover surface. */
	visualDraftEphemeral: boolean;
};

/** Interaction modes that unmount or gate page layers. */
export type PdfPageModeSlice = {
	regionSelecting: boolean;
	visualCropPending: boolean;
	visualDraftOpen: boolean;
};

export type PdfPageHandlers = {
	onOpenPin: (pin: SelectionPin) => void;
	onCardHoverEnter: () => void;
	onCardHoverLeave: () => void;
	onCitationActivate: (link: PdfLinkAnnoObject) => void;
	onCitationHover: (link: PdfLinkAnnoObject | null) => void;
	onRegionSelect: (page: number, region: PdfAskNormalizedRect) => void;
	onLayoutHoverEnter: (region: PdfLayoutRegion) => void;
	onLayoutHoverLeave: (regionId: string) => void;
	/** Click a figure/table/algorithm hit target → open the annotate editor. */
	onLayoutRegionClick: (region: PdfLayoutRegion) => void;
	onDraftHoverEnter: () => void;
	onDraftHoverLeave: () => void;
	onTogglePageLayoutTranslate: (pageIndex: number) => void;
	/** Delete a highlight annotation directly from its on-page selection menu. */
	onDeleteHighlightAnnotation: (pageIndex: number, id: string) => void;
	/** Open the note editor for a highlight from its on-page selection menu. */
	onEditHighlightAnnotation: (id: string) => void;
	/** Change the color of a highlight annotation from its on-page selection menu. */
	onChangeHighlightColor: (
		pageIndex: number,
		id: string,
		color: HighlightColor,
	) => void;
};

export type PdfPageLayersProps = {
	docId: string;
	pageIndex: number;
	width: number;
	height: number;
	pdfDark: boolean;
	/** Read at render time only; page width/height already track zoom. */
	zoomRef: RefObject<number>;
	marks: PdfPageMarksSlice;
	layout: PdfPageLayoutSlice;
	mode: PdfPageModeSlice;
	handlers: PdfPageHandlers;
};

type PageTranslateTabProps = {
	pageIndex: number;
	active: boolean;
	running: boolean;
	onToggle: (pageIndex: number) => void;
};

function labelCharacters(label: string): { key: string; char: string }[] {
	const seen = new Map<string, number>();
	return Array.from(label, (char) => {
		const count = (seen.get(char) ?? 0) + 1;
		seen.set(char, count);
		return { key: `${char}-${count}`, char };
	});
}

const PageTranslateTab = memo(function PageTranslateTab({
	pageIndex,
	active,
	running,
	onToggle,
}: PageTranslateTabProps) {
	const { t } = useTranslation("viewer");
	const label = running
		? t("pdf.layoutTranslate.pageRunning")
		: active
			? t("pdf.layoutTranslate.hidePage")
			: t("pdf.layoutTranslate.translatePage");
	const shortLabel = active
		? t("pdf.layoutTranslate.hidePageShort")
		: t("pdf.layoutTranslate.translatePageShort");
	const Icon = running ? Loader2 : active ? EyeOff : Languages;
	return (
		<button
			type="button"
			className={cn(
				"absolute top-3 right-0 z-[6] flex min-h-[72px] w-8 translate-x-full flex-col items-center justify-center gap-1 rounded-r-md border border-l-0 border-border/80 bg-background/95 px-1 py-2 font-medium text-[11px] text-foreground shadow-sm ring-1 ring-black/5 backdrop-blur-sm transition-colors hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/60 dark:ring-white/10",
				active && "border-primary/30 bg-primary/10 text-primary",
			)}
			aria-label={label}
			aria-pressed={active}
			onClick={(event) => {
				event.preventDefault();
				event.stopPropagation();
				onToggle(pageIndex);
			}}
			onPointerDown={(event) => event.stopPropagation()}
		>
			<Icon
				className={cn("size-3.5 shrink-0", running && "animate-spin")}
				aria-hidden="true"
			/>
			<span className="flex flex-col items-center gap-0.5 leading-none">
				{labelCharacters(shortLabel).map((part) => (
					<span key={part.key} className="block text-center">
						{part.char}
					</span>
				))}
			</span>
		</button>
	);
});

type LayoutRegionHitTargetProps = {
	region: PdfLayoutRegion;
	formulaLegend: boolean;
	ariaLabel: string;
	onHoverEnter: () => void;
	onHoverLeave: () => void;
	onClick: () => void;
};

/**
 * Layout regions are buttons so they remain keyboard accessible. Pointer
 * activation needs an extra gesture gate because a held pointer can move over
 * a region while the PDF is being scrolled.
 */
const LayoutRegionHitTarget = memo(function LayoutRegionHitTarget({
	region,
	formulaLegend,
	ariaLabel,
	onHoverEnter,
	onHoverLeave,
	onClick,
}: LayoutRegionHitTargetProps) {
	const guardRef = useRef(createLayoutRegionClickGuard());
	const cleanupRef = useRef<(() => void) | null>(null);

	const cleanupWindowListeners = () => {
		cleanupRef.current?.();
		cleanupRef.current = null;
	};

	useEffect(
		() => () => {
			cleanupRef.current?.();
			cleanupRef.current = null;
		},
		[],
	);

	return (
		<button
			type="button"
			data-layout-hit={region.id}
			aria-label={ariaLabel}
			className="absolute z-[2] cursor-pointer rounded-none border-0 bg-transparent p-0 transition-colors hover:bg-primary/5"
			style={{
				left: `${region.bbox.x * 100}%`,
				top: `${region.bbox.y * 100}%`,
				width: `${region.bbox.w * 100}%`,
				height: `${region.bbox.h * 100}%`,
			}}
			onPointerEnter={formulaLegend ? onHoverEnter : undefined}
			onPointerLeave={formulaLegend ? onHoverLeave : undefined}
			onPointerDown={(event) => {
				if (formulaLegend || event.button !== 0) return;
				cleanupWindowListeners();
				guardRef.current.begin(event.pointerId, event.clientX, event.clientY);
				event.currentTarget.setPointerCapture(event.pointerId);
				const invalidate = () => guardRef.current.invalidate();
				const ownerWindow = event.currentTarget.ownerDocument.defaultView;
				ownerWindow?.addEventListener("wheel", invalidate, {
					capture: true,
					passive: true,
				});
				ownerWindow?.addEventListener("scroll", invalidate, {
					capture: true,
					passive: true,
				});
				cleanupRef.current = () => {
					ownerWindow?.removeEventListener("wheel", invalidate, true);
					ownerWindow?.removeEventListener("scroll", invalidate, true);
				};
			}}
			onPointerMove={(event) => {
				if (formulaLegend) return;
				guardRef.current.move(event.pointerId, event.clientX, event.clientY);
			}}
			onPointerUp={(event) => {
				if (formulaLegend) return;
				guardRef.current.end(event.pointerId);
				cleanupWindowListeners();
			}}
			onPointerCancel={(event) => {
				if (formulaLegend) return;
				guardRef.current.invalidate();
				guardRef.current.end(event.pointerId);
				cleanupWindowListeners();
			}}
			onClick={(event) => {
				if (formulaLegend) return;
				// detail=0 is keyboard activation; pointer activation must pass
				// the movement / wheel / scroll gate above.
				if (event.detail === 0 || guardRef.current.consume()) {
					onClick();
				}
			}}
		/>
	);
});

export const PdfPageLayers = memo(function PdfPageLayers({
	docId,
	pageIndex,
	width,
	height,
	pdfDark,
	zoomRef,
	marks,
	layout,
	mode,
	handlers,
}: PdfPageLayersProps) {
	const { t } = useTranslation("viewer");
	const pageNumber = pageIndex + 1;
	const activeAskOnPage =
		marks.activeAskAnchor?.page === pageNumber ? marks.activeAskAnchor : null;
	const activeTranslateOnPage =
		marks.activeTranslateAnchor?.page === pageNumber
			? marks.activeTranslateAnchor
			: null;
	const activeVisualOnPage =
		marks.activeVisualTrace?.page === pageNumber
			? marks.activeVisualTrace
			: null;
	const visualDraftRegionOnPage =
		marks.visualDraftRegion?.page === pageNumber
			? marks.visualDraftRegion.region
			: null;
	const formulaAnnotationRegionOnPage =
		marks.formulaAnnotationRegion?.page === pageNumber
			? marks.formulaAnnotationRegion.region
			: null;
	const focusedLayoutOnPage =
		marks.focusedLayoutRegion?.pageIndex === pageIndex
			? marks.focusedLayoutRegion
			: null;
	const pins = marks.pinsByPage.get(pageNumber) ?? EMPTY_PINS;
	const layoutTranslateOnPage =
		layout.layoutTranslateItemsByPage.get(pageIndex);
	const pageTranslateState = layout.layoutTranslatePageStateByPage.get(
		pageIndex,
	) ?? { active: false, running: false };
	// Page shell: paper-white in light mode; near-black when PDF dark mode is on
	// so loading gaps match inverted page rasters.
	return (
		<div
			className={cn(
				"relative overflow-visible rounded-sm shadow-sm ring-1",
				pdfDark ? "bg-zinc-900 ring-white/10" : "bg-white ring-black/5",
			)}
			style={{ width, height }}
			{...{ [EMBED_PAGE_ATTR]: pageIndex }}
		>
			{/*
			 * EmbedPDF has no page color-scheme API yet (UI chrome theme only).
			 * Invert + hue-rotate only the raster layers so selection / search /
			 * annotation / pin overlays keep their intended colors. Agent crops
			 * use engine.renderPageRect and are unaffected.
			 */}
			<RenderLayer
				documentId={docId}
				pageIndex={pageIndex}
				scale={Math.min(zoomRef.current, PDF_BASE_LAYER_SCALE_CAP)}
				dpr={pdfRasterDpr()}
				className={pdfDark ? PDF_PAGE_RASTER_DARK_CLASS : undefined}
				style={PAGE_LAYER_STYLE}
			/>
			<TilingLayer
				documentId={docId}
				pageIndex={pageIndex}
				dpr={pdfRasterDpr()}
				className={pdfDark ? PDF_PAGE_RASTER_DARK_CLASS : undefined}
				style={PAGE_LAYER_STYLE}
			/>
			<SearchLayer
				documentId={docId}
				pageIndex={pageIndex}
				style={PAGE_LAYER_STYLE}
			/>
			{/*
			 * EmbedPDF raw bbox layer — kept mounted for plugin state, but
			 * visibility is forced off (see effect). Store-backed boxes below.
			 */}
			<LayoutAnalysisLayer
				documentId={docId}
				pageIndex={pageIndex}
				style={PAGE_LAYER_STYLE}
			/>
			<PagePointerProvider
				documentId={docId}
				pageIndex={pageIndex}
				style={PAGE_LAYER_STYLE}
			>
				{/* Unmount text selection while framing a visual region. */}
				{mode.regionSelecting ? null : (
					<SelectionLayer documentId={docId} pageIndex={pageIndex} />
				)}
				<AnnotationLayer
					documentId={docId}
					pageIndex={pageIndex}
					selectionMenu={(menuProps) => (
						<HighlightAnnotationMenu
							{...menuProps}
							onEdit={handlers.onEditHighlightAnnotation}
							onDelete={handlers.onDeleteHighlightAnnotation}
							onChangeColor={handlers.onChangeHighlightColor}
						/>
					)}
				/>
				<PageTranslateTab
					pageIndex={pageIndex}
					active={pageTranslateState.active}
					running={pageTranslateState.running}
					onToggle={handlers.onTogglePageLayoutTranslate}
				/>
				<CitationLinkLayer
					links={marks.citationLinks.get(pageIndex) ?? EMPTY_CITATION_LINKS}
					pageWidthPt={width / zoomRef.current}
					pageHeightPt={height / zoomRef.current}
					label={t("pdf.linkAria")}
					onActivate={handlers.onCitationActivate}
					onHover={handlers.onCitationHover}
				/>
				<PdfRegionSelectLayer
					active={mode.regionSelecting && !mode.visualCropPending}
					label={t("pdfExplain.regionSelectionLabel", {
						page: pageNumber,
					})}
					onSelect={(region) => handlers.onRegionSelect(pageNumber, region)}
				/>
				{/*
				 * Debug Eye overlay: pre-merge detections (all kinds, no NMS),
				 * score ≥ LAYOUT_SIDEBAR_MIN_SCORE (30%). Label = kind + conf.
				 */}
				{layout.layoutOverlayVisible
					? layout.rawRegionsByPage.get(pageIndex)?.map((region) => {
							const pct = Math.round(region.score * 100);
							const kindLabel = t(layoutKindI18nKey(region.kind));
							const label = t("figures.overlayLabel", {
								kind: kindLabel,
								pct,
							});
							return (
								<div
									key={`layout-box-${region.id}`}
									className="pointer-events-none absolute z-[1] rounded-none border"
									style={{
										left: `${region.bbox.x * 100}%`,
										top: `${region.bbox.y * 100}%`,
										width: `${region.bbox.w * 100}%`,
										height: `${region.bbox.h * 100}%`,
										borderColor: layoutKindBorder(region.kind),
										backgroundColor: layoutKindFill(region.kind),
									}}
									aria-hidden="true"
								>
									<span
										className="absolute top-0 left-0 max-w-full truncate rounded-br-sm px-1 py-px font-medium text-[10px] text-white leading-4"
										style={{
											backgroundColor: layoutKindHex(region.kind),
										}}
									>
										{label}
									</span>
								</div>
							);
						})
					: null}
				{/* Bulk layout translate: progressive text overlays over body blocks. */}
				{layoutTranslateOnPage && layoutTranslateOnPage.length > 0 ? (
					<LayoutTranslateOverlay
						items={layoutTranslateOnPage}
						pageWidthPx={width}
						pageHeightPx={height}
						pdfDark={pdfDark}
					/>
				) : null}
				{/*
				 * Hit targets for post-merge figure/table/algorithm/formula.
				 * Largest first so smaller boxes stack on top and win pointer hits.
				 * Hidden when framing or a visual draft is open (not during crop:
				 * unmount leave must not cancel an in-flight crop).
				 * Formula legend keeps hits mounted so leave/enter can switch
				 * equations and drive hide without a second hover surface.
				 * Figures / tables / algorithms annotate on click; formulas keep
				 * the dwell-triggered glossary legend.
				 */}
				{!mode.regionSelecting && !mode.visualDraftOpen
					? layout.hoverableRegionsByPage.get(pageIndex)?.map((region) => {
							const formulaLegend =
								isFormulaLayoutKind(region.kind) &&
								layout.equationSymbolCount > 0;
							return (
								<LayoutRegionHitTarget
									key={`layout-hit-${region.id}`}
									region={region}
									formulaLegend={formulaLegend}
									ariaLabel={
										formulaLegend
											? t("equationAnnotation.hoverAria")
											: t("figures.clickAnnotateAria")
									}
									onHoverEnter={() => handlers.onLayoutHoverEnter(region)}
									onHoverLeave={() => handlers.onLayoutHoverLeave(region.id)}
									onClick={() => handlers.onLayoutRegionClick(region)}
								/>
							);
						})
					: null}
				{/* Open ask conversation card: highlight the anchored selection. */}
				{activeAskOnPage
					? activeAskOnPage.rects.map((rect) => (
							<div
								key={`${activeAskOnPage.id}-source-${rect.x}-${rect.y}-${rect.w}-${rect.h}`}
								className="pointer-events-auto absolute z-[1] rounded-[2px] bg-amber-300/45 dark:bg-amber-400/35"
								style={{
									left: `${rect.x * 100}%`,
									top: `${rect.y * 100}%`,
									width: `${rect.w * 100}%`,
									height: `${rect.h * 100}%`,
								}}
								aria-hidden="true"
								onMouseEnter={handlers.onCardHoverEnter}
								onMouseLeave={handlers.onCardHoverLeave}
							/>
						))
					: null}
				{activeTranslateOnPage
					? activeTranslateOnPage.rects.map((rect) => (
							<div
								key={`${activeTranslateOnPage.id}-source-${rect.x}-${rect.y}-${rect.w}-${rect.h}`}
								className="pointer-events-auto absolute z-[1] rounded-[2px] bg-yellow-300/40 dark:bg-yellow-400/35"
								style={{
									left: `${rect.x * 100}%`,
									top: `${rect.y * 100}%`,
									width: `${rect.w * 100}%`,
									height: `${rect.h * 100}%`,
								}}
								aria-hidden="true"
								onMouseEnter={handlers.onCardHoverEnter}
								onMouseLeave={handlers.onCardHoverLeave}
							/>
						))
					: null}
				{/* Open visual draft / mark: show the framed source region on-page. */}
				{visualDraftRegionOnPage ? (
					<div
						className={cn(
							"absolute z-[2] rounded-none border border-primary/40 bg-primary/5 shadow-[0_0_0_1px_rgba(255,255,255,0.55)] dark:shadow-[0_0_0_1px_rgba(0,0,0,0.5)]",
							// Ephemeral layout-hover drafts need a hover surface so
							// leaving the region can schedule auto-hide.
							layout.visualDraftEphemeral
								? "pointer-events-auto"
								: "pointer-events-none",
						)}
						style={{
							left: `${visualDraftRegionOnPage.x * 100}%`,
							top: `${visualDraftRegionOnPage.y * 100}%`,
							width: `${visualDraftRegionOnPage.w * 100}%`,
							height: `${visualDraftRegionOnPage.h * 100}%`,
						}}
						aria-hidden="true"
						onMouseEnter={
							layout.visualDraftEphemeral
								? handlers.onDraftHoverEnter
								: undefined
						}
						onMouseLeave={
							layout.visualDraftEphemeral
								? handlers.onDraftHoverLeave
								: undefined
						}
					/>
				) : null}
				{/*
				 * Formula legend: keep the same primary visual frame as visual-ask
				 * so the hovered equation is clearly boxed on the page.
				 * Hits own enter/leave; frame is visual-only.
				 */}
				{formulaAnnotationRegionOnPage ? (
					<div
						className="pointer-events-none absolute z-[2] rounded-none border border-primary/40 bg-primary/5 shadow-[0_0_0_1px_rgba(255,255,255,0.55)] dark:shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
						style={{
							left: `${formulaAnnotationRegionOnPage.x * 100}%`,
							top: `${formulaAnnotationRegionOnPage.y * 100}%`,
							width: `${formulaAnnotationRegionOnPage.w * 100}%`,
							height: `${formulaAnnotationRegionOnPage.h * 100}%`,
						}}
						aria-hidden="true"
					/>
				) : null}
				{/* Figures sidebar selection: EmbedPDF layout hue for kind. */}
				{focusedLayoutOnPage && !formulaAnnotationRegionOnPage ? (
					<div
						className="pointer-events-none absolute z-[2] rounded-none border shadow-[0_0_0_1px_rgba(255,255,255,0.55)] dark:shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
						style={{
							left: `${focusedLayoutOnPage.bbox.x * 100}%`,
							top: `${focusedLayoutOnPage.bbox.y * 100}%`,
							width: `${focusedLayoutOnPage.bbox.w * 100}%`,
							height: `${focusedLayoutOnPage.bbox.h * 100}%`,
							borderColor: layoutKindHex(focusedLayoutOnPage.kind),
							backgroundColor: layoutKindFill(focusedLayoutOnPage.kind),
							// Keep a slightly stronger edge for visibility.
							outline: `1px solid ${layoutKindBorder(focusedLayoutOnPage.kind)}`,
						}}
						aria-hidden="true"
					/>
				) : null}
				{/* Active visual mark: theme outline of the crop region. */}
				{activeVisualOnPage
					? activeVisualOnPage.rects.map((rect) => (
							<div
								key={`${activeVisualOnPage.id}-region-${rect.x}-${rect.y}-${rect.w}-${rect.h}`}
								className="pointer-events-none absolute z-[2] rounded-none border border-primary/40 bg-primary/5 shadow-[0_0_0_1px_rgba(255,255,255,0.55)] dark:shadow-[0_0_0_1px_rgba(0,0,0,0.5)]"
								style={{
									left: `${rect.x * 100}%`,
									top: `${rect.y * 100}%`,
									width: `${rect.w * 100}%`,
									height: `${rect.h * 100}%`,
								}}
								aria-hidden="true"
							/>
						))
					: null}
				<SelectionGutter
					items={pins}
					activeId={marks.activeCardId}
					onOpen={handlers.onOpenPin}
					onEnter={handlers.onCardHoverEnter}
					onLeave={handlers.onCardHoverLeave}
				/>
			</PagePointerProvider>
		</div>
	);
});
