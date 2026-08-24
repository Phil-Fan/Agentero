/**
 * Public contract of the EmbedPDF viewer plus the local card/editor state
 * shapes its features pass around.
 */

import type { FormattedSelection } from "@embedpdf/plugin-selection/react";
import type { PromptImage } from "@/lib/agent/api";
import type { Citation } from "@/lib/paper/refs";
import type { PdfVisualSessionTrace } from "@/lib/pdf/agent-trace";
import type {
	PdfAskAnchor,
	PdfAskNormalizedRect,
	PdfAskThread,
} from "@/lib/pdf/ask";
import type { CrossrefKind } from "@/lib/pdf/citation-dest-keys";
import type { HighlightColor } from "@/lib/pdf/highlight/palette";
import type { PdfHighlight } from "@/lib/pdf/highlight/types";
import type { PdfViewerHandle } from "@/lib/workspace/viewer/pdf-viewer-registry";

export type { PdfViewerHandle };

export type PdfViewerProps = {
	/**
	 * PDF source: local `blob:` (bytes via fs) or remote https. Prefer local
	 * vault PDF; remote URL is fallback when download fails.
	 */
	source: string | null;
	/**
	 * Local PDF bytes. Preferred over `source`: the engine opens the document
	 * straight from the buffer, avoiding a `fetch(blob:)` that stalls/fails in
	 * some webviews (Windows WebView2). `source` is the fallback (remote https).
	 */
	sourceBytes?: ArrayBuffer | null;
	/** Stable per-tab document id (EmbedPDF documentId + scope key). */
	docId?: string | null;
	/** Absolute path to paper folder for annotations/marks persistence */
	paperAbsPath?: string | null;
	/** Vault-relative paper path stored inside JSON */
	paperRelPath?: string | null;
	/** Current vault root for ACP cwd */
	vaultPath?: string | null;
	/** Open Translate settings from a translation error card. */
	onOpenSettings?: () => void;
	className?: string;
	/** Register/unregister an imperative handle for the annotations panel */
	onHandle?: (handle: PdfViewerHandle | null) => void;
	/** Called whenever the highlight list changes (for the annotations panel) */
	onHighlightsChange?: (highlights: PdfHighlight[]) => void;
	/** Called whenever PDF ask threads change (for the annotations panel) */
	onAsksChange?: (threads: PdfAskThread[]) => void;
	/** Called whenever visual agent-trace marks change (for the annotations panel) */
	onVisualTracesChange?: (traces: PdfVisualSessionTrace[]) => void;
	/**
	 * Workspace active tab. Dock may keep inactive PDFs mounted (`keepMounted`);
	 * only the active viewer should refresh marks/ (expensive base64 JSON list).
	 */
	isActive?: boolean;
};

export type PdfViewerInnerProps = PdfViewerProps & { docId: string };

/** Viewport-space point (client px) used by every floating overlay. */
export type ScreenPoint = {
	x: number;
	y: number;
};

/** Screen anchor for a floating card, including which side to open on. */
export type CardScreenPoint = ScreenPoint & {
	preferRight?: boolean;
};

export type SelectionMenuState = {
	screen: ScreenPoint;
	anchor: PdfAskAnchor;
	pages: FormattedSelection[];
};

export type CitationPreviewState = {
	screen: ScreenPoint;
	/** Sidecar citation the hovered link points at (exact hyperref lookup). */
	matched: Citation;
};

/**
 * Hover card for a `\ref` cross-reference link: a crop of the figure / table /
 * equation / algorithm the link points at, resolved through the hyperref
 * cross-reference destination map plus layout regions. `image` is null while
 * the region crop renders.
 */
export type CrossrefPreviewState = {
	screen: ScreenPoint;
	kind: CrossrefKind;
	/** 1-based destination page (display only). */
	page: number;
	/** Normalized bbox of the resolved layout region on the destination page. */
	region: PdfAskNormalizedRect;
	/** Rendered region crop; null while in flight. */
	image: PromptImage | null;
};

export type EditorState = {
	screen: ScreenPoint;
	pageIndex: number;
	id: string;
	comment: string;
};

export type VisualDraftEditorState = {
	screen: ScreenPoint;
	page: number;
	region: PdfAskNormalizedRect;
	image: PromptImage;
};

/** Persistent comment-rail card for one annotated highlight (per page). */
export type PageAnnotationComment = {
	id: string;
	/** Normalized Y anchor on the page (0-1) used for initial placement. */
	anchorY: number;
	quote: string;
	comment: string;
	color: HighlightColor;
	/** Pre-computed `[[alias|target]]` or null if no wiki target. */
	linkAlias: string | null;
};
