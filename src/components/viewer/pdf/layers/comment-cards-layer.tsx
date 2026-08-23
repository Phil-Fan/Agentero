/**
 * Feishu-style comment rail: one persistent card per annotated highlight,
 * pinned just outside the page's right edge (`left: 100%` inside the
 * overflow-visible page container, same trick as PageTranslateTab). Cards
 * stack vertically with collision avoidance and clamp into the page height.
 */

import { Link2, Trash2 } from "lucide-react";
import { memo } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import type { PageAnnotationComment } from "@/components/viewer/pdf/types";
import { cn } from "@/lib/core/utils";
import {
	swatchBorderClass,
	swatchColorClass,
} from "@/lib/pdf/highlight/palette";

/** Card width (`w-56`) — also the gutter width reserved on the viewport. */
export const COMMENT_CARD_WIDTH_PX = 224;
/** Horizontal gap between the page edge and the rail. */
export const COMMENT_CARD_GAP_PX = 8;
/** Viewport right padding that keeps the rail clear of horizontal scroll. */
export const COMMENT_RAIL_WIDTH_PX =
	COMMENT_CARD_WIDTH_PX + COMMENT_CARD_GAP_PX;
/** Hosts narrower than this fall back to the in-page annotate gutter pins. */
export const COMMENT_RAIL_MIN_HOST_WIDTH = 640;

const CARD_GAP_PX = 8;
/** text-xs leading-relaxed ≈ 12px × 1.625. */
const CARD_LINE_HEIGHT_PX = 20;
/** Conservative chars per line at w-56 with padding (CJK-heavy notes). */
const CARD_CHARS_PER_LINE = 15;
/** Padding + color-dot row + blockquote/comment margins. */
const CARD_BASE_HEIGHT_PX = 54;

type CommentCardsLayerProps = {
	/** Comments for this page only. */
	items: PageAnnotationComment[];
	/** Rendered page height in px (zoom-aware). */
	pageHeightPx: number;
	pageIndex: number;
	/** Resolvable wiki target; copy buttons only render when set. */
	wikiTarget: string | null;
	onOpen: (id: string) => void;
	onDelete: (pageIndex: number, id: string) => void;
	onCopyLink: (comment: PageAnnotationComment) => void;
	onCopyEmbed: (comment: PageAnnotationComment) => void;
};

export type CommentCardPlacement = {
	id: string;
	topPx: number;
	heightPx: number;
};

/** Visual line count after clamping (quote: 2, comment: 3). */
function clampedLines(text: string, max: number): number {
	let lines = 0;
	for (const raw of text.split("\n")) {
		lines += Math.max(1, Math.ceil(raw.length / CARD_CHARS_PER_LINE));
		if (lines >= max) return max;
	}
	return Math.max(1, lines);
}

/** Conservative card height estimate from clamped quote/comment lines. */
function estimateCommentCardHeight(item: PageAnnotationComment): number {
	const quoteLines = item.quote.trim() ? clampedLines(item.quote, 2) : 0;
	const commentLines = clampedLines(item.comment, 3);
	return (
		CARD_BASE_HEIGHT_PX + (quoteLines + commentLines) * CARD_LINE_HEIGHT_PX
	);
}

/**
 * Anchor each card at its highlight height, then nudge overlapping cards
 * downward (never sideways) and clamp the whole stack into the page height.
 */
export function layoutCommentCards(
	items: PageAnnotationComment[],
	pageHeightPx: number,
): CommentCardPlacement[] {
	const sorted = [...items].sort(
		(a, b) => a.anchorY - b.anchorY || a.id.localeCompare(b.id),
	);
	const laid: CommentCardPlacement[] = [];

	for (const item of sorted) {
		const heightPx = estimateCommentCardHeight(item);
		const anchorTop = item.anchorY * pageHeightPx;
		const prev = laid[laid.length - 1];
		const topPx = Math.max(
			anchorTop,
			prev ? prev.topPx + prev.heightPx + CARD_GAP_PX : 0,
		);
		laid.push({ id: item.id, topPx, heightPx });
	}

	// Clamp the stack into the page: shift cards up bottom-first, keeping the
	// avoidance gap between neighbours.
	for (let i = laid.length - 1; i >= 0; i -= 1) {
		const card = laid[i];
		const next = laid[i + 1];
		const maxTop = next
			? next.topPx - CARD_GAP_PX - card.heightPx
			: pageHeightPx - card.heightPx;
		card.topPx = Math.max(0, Math.min(card.topPx, maxTop));
	}

	return laid;
}

export const CommentCardsLayer = memo(function CommentCardsLayer({
	items,
	pageHeightPx,
	pageIndex,
	wikiTarget,
	onOpen,
	onDelete,
	onCopyLink,
	onCopyEmbed,
}: CommentCardsLayerProps) {
	const { t } = useTranslation("viewer");
	if (!items.length) return null;

	const laid = layoutCommentCards(items, pageHeightPx);
	const byId = new Map(items.map((item) => [item.id, item]));

	return (
		<div className="pointer-events-none absolute inset-0 z-[5]">
			<TooltipProvider delayDuration={200}>
				{laid.map((pos) => {
					const item = byId.get(pos.id);
					if (!item) return null;
					return (
						<div
							key={item.id}
							className="group pointer-events-auto absolute w-56 rounded-lg bg-background/95 px-2.5 py-2 shadow-sm ring-1 ring-border/60 backdrop-blur-sm"
							style={{
								left: `calc(100% + ${COMMENT_CARD_GAP_PX}px)`,
								top: pos.topPx,
							}}
						>
							{/* biome-ignore lint/a11y/useSemanticElements: a native <button> cannot wrap the blockquote/p flow content */}
							<div
								role="button"
								tabIndex={0}
								className="block w-full cursor-pointer rounded-md text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
								onClick={(e) => {
									e.stopPropagation();
									onOpen(item.id);
								}}
								onKeyDown={(e) => {
									if (e.key === "Enter" || e.key === " ") {
										e.preventDefault();
										onOpen(item.id);
									}
								}}
							>
								<span
									className={cn(
										"block size-2 rounded-full",
										swatchColorClass(item.color),
									)}
									aria-hidden
								/>
								{item.quote.trim() ? (
									<blockquote
										className={cn(
											"mt-1.5 line-clamp-2 border-l-2 pl-2 text-muted-foreground/90 text-xs leading-relaxed",
											swatchBorderClass(item.color),
										)}
									>
										{item.quote}
									</blockquote>
								) : null}
								<p className="mt-1 line-clamp-3 whitespace-pre-wrap break-words text-[13px] text-foreground/80 leading-relaxed">
									{item.comment}
								</p>
							</div>
							<div className="absolute top-1.5 right-1.5 flex items-center gap-0.5 rounded-lg bg-background/80 p-0.5 opacity-0 shadow-sm ring-1 ring-border/60 backdrop-blur-sm transition-opacity duration-150 group-hover:opacity-100 group-focus-within:opacity-100">
								{wikiTarget ? (
									<>
										<Tooltip>
											<TooltipTrigger asChild>
												<Button
													type="button"
													variant="ghost"
													size="icon-xs"
													className="size-6 text-muted-foreground hover:text-foreground"
													aria-label={t("annotations.copyLink")}
													onClick={(e) => {
														e.stopPropagation();
														onCopyLink(item);
													}}
												>
													<Link2 className="size-3.5" />
												</Button>
											</TooltipTrigger>
											<TooltipContent>
												{t("annotations.copyLink")}
											</TooltipContent>
										</Tooltip>
										<Tooltip>
											<TooltipTrigger asChild>
												<Button
													type="button"
													variant="ghost"
													size="icon-xs"
													className="size-6 text-muted-foreground hover:text-foreground"
													aria-label={t("annotations.copyEmbed")}
													onClick={(e) => {
														e.stopPropagation();
														onCopyEmbed(item);
													}}
												>
													<span className="font-mono text-[10px] leading-none">
														![[
													</span>
												</Button>
											</TooltipTrigger>
											<TooltipContent>
												{t("annotations.copyEmbed")}
											</TooltipContent>
										</Tooltip>
									</>
								) : null}
								<Tooltip>
									<TooltipTrigger asChild>
										<Button
											type="button"
											variant="ghost"
											size="icon-xs"
											className="size-6 text-muted-foreground hover:text-destructive"
											aria-label={t("annotations.delete")}
											onClick={(e) => {
												e.stopPropagation();
												onDelete(pageIndex, item.id);
											}}
										>
											<Trash2 className="size-3.5" />
										</Button>
									</TooltipTrigger>
									<TooltipContent>{t("annotations.delete")}</TooltipContent>
								</Tooltip>
							</div>
						</div>
					);
				})}
			</TooltipProvider>
		</div>
	);
});
