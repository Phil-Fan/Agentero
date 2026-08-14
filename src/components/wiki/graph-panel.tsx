import { Focus, LibraryBig, Network } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type ForceGraph2D from "react-force-graph-2d";
import { useTranslation } from "react-i18next";

import { PaneHeader } from "@/components/shell/pane-header";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/core/utils";
import {
	type CiteGraphNode,
	type CiteGraphResponse,
	type CiteGraphRole,
	paperRefsGraph,
} from "@/lib/paper/refs";

type GraphPanelProps = {
	vaultPath: string | null;
	selectedPath: string | null;
	onOpenPath: (vaultRelativePath: string) => void;
	className?: string;
	/**
	 * Optional revision bump to force a re-fetch (e.g. after import / reparse).
	 * Citation graph does not depend on the wiki index.
	 */
	wikiIndexRevision?: number;
	/**
	 * Embedded under References: denser chrome.
	 */
	embedded?: boolean;
};

/** Force-graph mutates x/y at runtime; only declare what we paint/read. */
type FgNode = CiteGraphNode & {
	x?: number;
	y?: number;
};

type FgLink = {
	source: string | FgNode;
	target: string | FgNode;
	id: string;
};

/** Graph mode: neighborhood around the selected paper, or full library. */
type GraphMode = "center" | "all";

/** Theme-derived colors for canvas (resolved from CSS variables). */
type ThemeColors = {
	foreground: string;
	mutedForeground: string;
	border: string;
	primary: string;
	muted: string;
	brand: string;
	highlight: string;
	chart3: string;
};

function readThemeColors(el: HTMLElement | null): ThemeColors {
	const fallback: ThemeColors = {
		foreground: "var(--foreground)",
		mutedForeground: "var(--muted-foreground)",
		border: "var(--border)",
		primary: "var(--primary)",
		muted: "var(--muted)",
		brand: "var(--brand)",
		highlight: "var(--highlight)",
		chart3: "var(--chart-3)",
	};
	if (
		typeof document === "undefined" ||
		typeof getComputedStyle === "undefined"
	) {
		return fallback;
	}
	const style = getComputedStyle(el ?? document.documentElement);
	const pick = (name: string, fallback: string) => {
		const v = style.getPropertyValue(name).trim();
		return v || fallback;
	};
	// Prefer resolved color tokens already used by the UI
	return {
		foreground: pick("--foreground", fallback.foreground),
		mutedForeground: pick("--muted-foreground", fallback.mutedForeground),
		border: pick("--border", fallback.border),
		primary: pick("--primary", fallback.primary),
		muted: pick("--muted", fallback.muted),
		brand: pick("--brand", fallback.brand),
		highlight: pick("--highlight", fallback.highlight),
		chart3: pick("--chart-3", fallback.chart3),
	};
}

/**
 * Four-tier visual encoding (neighborhood mode):
 * 1. center — brand circle + ring; 2. reference (cited, in library) — circle;
 * 3. citedBy (citing, in library) — square; 4. stub (not in library) — diamond.
 * Full-library mode has no roles: papers are circles, the selected paper gets
 * the center treatment.
 */
type NodeVisual = {
	shape: "circle" | "square" | "diamond";
	fill: string;
	ring: boolean;
};

function nodeVisual(node: FgNode, colors: ThemeColors): NodeVisual {
	if (node.role === "center") {
		return { shape: "circle", fill: colors.brand, ring: true };
	}
	if (node.type === "stub") {
		return { shape: "diamond", fill: colors.highlight, ring: false };
	}
	if (node.role === "citedBy") {
		return { shape: "square", fill: colors.chart3, ring: false };
	}
	return { shape: "circle", fill: colors.foreground, ring: false };
}

/** Radius for a paper node scales with incoming citation count. */
function nodeRadius(node: FgNode, cited: number): number {
	if (node.type !== "paper") return 4;
	const base = 5.5 + Math.min(Math.sqrt(cited) * 1.4, 6.5);
	return node.role === "center" ? base + 1.5 : base;
}

function traceShape(
	ctx: CanvasRenderingContext2D,
	shape: NodeVisual["shape"],
	x: number,
	y: number,
	r: number,
) {
	if (shape === "circle") {
		ctx.arc(x, y, r, 0, 2 * Math.PI, false);
		return;
	}
	if (shape === "square") {
		const s = r * 0.9;
		ctx.rect(x - s, y - s, 2 * s, 2 * s);
		return;
	}
	// diamond
	ctx.moveTo(x, y - r * 1.15);
	ctx.lineTo(x + r * 1.15, y);
	ctx.lineTo(x, y + r * 1.15);
	ctx.lineTo(x - r * 1.15, y);
	ctx.closePath();
}

export function GraphPanel({
	vaultPath,
	selectedPath,
	onOpenPath,
	className,
	wikiIndexRevision = 0,
	embedded = false,
}: GraphPanelProps) {
	const { t } = useTranslation("sidebar");
	const wrapRef = useRef<HTMLDivElement>(null);
	const [size, setSize] = useState({ w: 280, h: 160 });
	const [data, setData] = useState<CiteGraphResponse | null>(null);
	const [loading, setLoading] = useState(false);
	const [error, setError] = useState<string | null>(null);
	const [hoverId, setHoverId] = useState<string | null>(null);
	const [mode, setMode] = useState<GraphMode>("center");
	const [forceGraph, setForceGraph] = useState<typeof ForceGraph2D | null>(
		null,
	);
	const [colors, setColors] = useState<ThemeColors>(() =>
		readThemeColors(null),
	);
	const ForceGraph = forceGraph;

	const centerActive = mode === "center" && Boolean(selectedPath);

	useEffect(() => {
		const el = wrapRef.current;
		if (!el || typeof ResizeObserver === "undefined") return;
		const ro = new ResizeObserver((entries) => {
			const cr = entries[0]?.contentRect;
			if (!cr) return;
			setSize({
				w: Math.max(120, Math.floor(cr.width)),
				h: Math.max(160, Math.floor(cr.height)),
			});
		});
		ro.observe(el);
		return () => ro.disconnect();
	}, []);

	// Keep canvas colors in sync with light/dark theme
	useEffect(() => {
		if (
			typeof document === "undefined" ||
			typeof MutationObserver === "undefined"
		) {
			return;
		}
		const el = wrapRef.current ?? document.documentElement;
		const sync = () => setColors(readThemeColors(el));
		sync();
		const mo = new MutationObserver(sync);
		mo.observe(document.documentElement, {
			attributes: true,
			attributeFilter: ["class", "style", "data-theme"],
		});
		return () => mo.disconnect();
	}, []);

	useEffect(() => {
		if (typeof window === "undefined") return;
		let cancelled = false;
		void import("react-force-graph-2d")
			.then(({ default: Component }) => {
				if (!cancelled) {
					setForceGraph(() => Component);
				}
			})
			.catch((e) => {
				if (!cancelled) {
					setError(e instanceof Error ? e.message : String(e));
				}
			});
		return () => {
			cancelled = true;
		};
	}, []);

	useEffect(() => {
		// Optional external refresh signal (import / vault switch).
		void wikiIndexRevision;
		let cancelled = false;
		setLoading(true);
		setError(null);
		void (async () => {
			try {
				const res = await paperRefsGraph(
					vaultPath,
					centerActive ? { center: selectedPath } : undefined,
				);
				if (cancelled) return;
				setData(res);
			} catch (e) {
				if (cancelled) return;
				setData(null);
				setError(e instanceof Error ? e.message : String(e));
			} finally {
				if (!cancelled) setLoading(false);
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [vaultPath, wikiIndexRevision, selectedPath, centerActive]);

	const graphData = useMemo(() => {
		if (!data) return { nodes: [] as FgNode[], links: [] as FgLink[] };
		return {
			nodes: data.nodes.map((n) => ({ ...n })),
			links: data.edges.map((e) => ({
				id: e.id,
				source: e.source,
				target: e.target,
			})),
		};
	}, [data]);

	// Incoming citation count per node: more-cited papers get larger.
	const citedCounts = useMemo(() => {
		const counts = new Map<string, number>();
		if (!data) return counts;
		for (const edge of data.edges) {
			counts.set(edge.target, (counts.get(edge.target) ?? 0) + 1);
		}
		return counts;
	}, [data]);

	// Neighborhood center; in full-library mode fall back to the selected paper
	// so it stays highlighted there too.
	const highlightId = data?.center ?? selectedPath ?? null;

	const openNode = useCallback(
		(node: CiteGraphNode) => {
			if (node.type === "stub" || !node.path) return;
			onOpenPath(node.path);
		},
		[onOpenPath],
	);

	const paintNode = useCallback(
		(node: FgNode, ctx: CanvasRenderingContext2D, globalScale: number) => {
			const cited = citedCounts.get(node.id) ?? 0;
			const r = nodeRadius(node, cited);
			const x = node.x ?? 0;
			const y = node.y ?? 0;
			const isCenter = node.id === highlightId;
			const isHover = node.id === hoverId;
			const dimmed = Boolean(hoverId) && !isHover && !isCenter;
			const visual = nodeVisual(
				isCenter && node.role !== "center"
					? { ...node, role: "center" as CiteGraphRole }
					: node,
				colors,
			);

			ctx.beginPath();
			traceShape(ctx, visual.shape, x, y, visual.ring ? r + 2 : r);
			ctx.fillStyle = visual.fill;
			ctx.globalAlpha = dimmed ? 0.28 : 1;
			ctx.fill();
			if (visual.ring) {
				ctx.strokeStyle = colors.foreground;
				ctx.lineWidth = 1.5 / globalScale;
				ctx.globalAlpha = 1;
				ctx.stroke();
			}

			let label = node.label;
			const maxChars = 28;
			if (label.length > maxChars) {
				label = `${label.slice(0, maxChars - 1)}…`;
			}
			const fontSize = 10 / globalScale;
			ctx.font = `${fontSize}px sans-serif`;
			ctx.textAlign = "center";
			ctx.textBaseline = "top";
			ctx.fillStyle = colors.mutedForeground;
			ctx.globalAlpha = dimmed ? 0.35 : 1;
			ctx.fillText(label, x, y + r + 2 / globalScale);
			ctx.globalAlpha = 1;
		},
		[highlightId, hoverId, colors, citedCounts],
	);

	const linkColor = useCallback(() => {
		// border token — quiet edges that follow theme
		return colors.border;
	}, [colors]);

	return (
		<div
			className={cn(
				"flex h-full min-h-0 flex-col overflow-hidden bg-background",
				className,
			)}
		>
			<PaneHeader
				className={embedded ? "h-7 min-h-7 px-2" : undefined}
				trailing={
					selectedPath ? (
						<TooltipProvider delayDuration={300}>
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										className="size-6 text-muted-foreground hover:text-foreground"
										aria-label={t(
											centerActive ? "graph.modeAll" : "graph.modeCenter",
										)}
										onClick={() =>
											setMode((m) => (m === "center" ? "all" : "center"))
										}
									>
										{centerActive ? (
											<LibraryBig className="size-3.5" aria-hidden />
										) : (
											<Focus className="size-3.5" aria-hidden />
										)}
									</Button>
								</TooltipTrigger>
								<TooltipContent>
									{t(centerActive ? "graph.modeAll" : "graph.modeCenter")}
								</TooltipContent>
							</Tooltip>
						</TooltipProvider>
					) : null
				}
			>
				<Network
					className="size-3.5 shrink-0 text-muted-foreground"
					aria-hidden
				/>
				<span
					className={cn(
						"min-w-0 flex-1 truncate font-medium leading-none",
						embedded ? "text-xs" : "text-sm",
					)}
				>
					{t("graph.title")}
				</span>
			</PaneHeader>

			<div ref={wrapRef} className="relative min-h-0 flex-1 bg-background">
				{loading ? (
					<p className="absolute inset-0 flex items-center justify-center text-muted-foreground text-xs">
						{t("graph.loading")}
					</p>
				) : null}
				{error ? (
					<p className="absolute inset-0 flex items-center justify-center px-3 text-center text-destructive text-xs">
						{error}
					</p>
				) : null}
				{!loading && !error && graphData.nodes.length === 0 ? (
					<p className="absolute inset-0 flex items-center justify-center px-3 text-center text-muted-foreground text-xs">
						{t("graph.noEdges")}
					</p>
				) : null}
				{!loading && !error && graphData.nodes.length > 0 && ForceGraph ? (
					<ForceGraph
						width={size.w}
						height={size.h}
						graphData={graphData}
						nodeId="id"
						linkSource="source"
						linkTarget="target"
						backgroundColor="rgba(0,0,0,0)"
						linkColor={linkColor}
						linkWidth={1}
						linkDirectionalArrowLength={3.5}
						linkDirectionalArrowRelPos={1}
						nodeCanvasObject={paintNode}
						nodePointerAreaPaint={(node, color, ctx) => {
							const n = node as FgNode;
							const cited = citedCounts.get(n.id) ?? 0;
							ctx.beginPath();
							ctx.arc(
								n.x ?? 0,
								n.y ?? 0,
								nodeRadius(n, cited) + 1,
								0,
								2 * Math.PI,
								false,
							);
							ctx.fillStyle = color;
							ctx.fill();
						}}
						onNodeHover={(node) => {
							setHoverId(node ? (node as FgNode).id : null);
						}}
						onNodeClick={(node) => {
							openNode(node as FgNode);
						}}
						cooldownTicks={80}
						enableNodeDrag
					/>
				) : null}
				{!loading && !error && centerActive && graphData.nodes.length > 0 ? (
					<div className="pointer-events-none absolute right-2 bottom-1 left-2 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[10px] text-muted-foreground">
						<span className="flex items-center gap-1">
							<span
								className="size-2 shrink-0 rounded-full"
								style={{ backgroundColor: colors.brand }}
								aria-hidden
							/>
							{t("graph.legend.center")}
						</span>
						<span className="flex items-center gap-1">
							<span
								className="size-2 shrink-0 rounded-full bg-foreground"
								aria-hidden
							/>
							{t("graph.legend.reference")}
						</span>
						<span className="flex items-center gap-1">
							<span
								className="size-2 shrink-0"
								style={{ backgroundColor: colors.chart3 }}
								aria-hidden
							/>
							{t("graph.legend.citedBy")}
						</span>
						<span className="flex items-center gap-1">
							<span
								className="size-2 shrink-0 rotate-45"
								style={{ backgroundColor: colors.highlight }}
								aria-hidden
							/>
							{t("graph.legend.external")}
						</span>
					</div>
				) : null}
			</div>
		</div>
	);
}
