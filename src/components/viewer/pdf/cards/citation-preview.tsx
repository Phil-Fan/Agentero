import { ArrowUpRight, BookCheck, Import, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";
import { CitationImportPopover } from "@/components/viewer/citation-import-menu";
import type { ScreenPoint } from "@/components/viewer/pdf/types";
import { openExternalUrl } from "@/lib/core/open-external";
import type { Citation } from "@/lib/paper/refs";
import {
	citationExternalUrl,
	citationImportIdentifier,
} from "@/lib/paper/refs";

const CARD_WIDTH = 300;
const CARD_ESTIMATED_HEIGHT = 110;

export type CitationPreviewImportMenu = {
	folders: string[];
	lastImportParentDir: string;
	importing: boolean;
	onImport: (citation: Citation, folder: string) => void;
	/** Lets the hover card stay open while the folder picker is up. */
	onOpenChange: (open: boolean) => void;
};

/**
 * Hover card for an in-text citation link: the reference it points at, resolved
 * exactly through the hyperref cite-key map. Only mounted when a match exists.
 * Header mirrors the References panel: in-library badge or import picker, plus
 * an external link.
 */
export function PdfCitationPreview({
	screen,
	matched,
	importMenu,
	onPointerEnter,
	onPointerLeave,
}: {
	screen: ScreenPoint;
	matched: Citation;
	importMenu?: CitationPreviewImportMenu;
	onPointerEnter: () => void;
	onPointerLeave: () => void;
}) {
	const { t } = useTranslation("viewer");
	const viewportWidth =
		typeof window === "undefined" ? 1200 : window.innerWidth;
	const viewportHeight =
		typeof window === "undefined" ? 800 : window.innerHeight;
	const left = Math.min(
		Math.max(12, screen.x),
		viewportWidth - CARD_WIDTH - 12,
	);
	const top = Math.min(
		Math.max(12, screen.y),
		viewportHeight - CARD_ESTIMATED_HEIGHT - 12,
	);
	const m = matched.metadata;
	const metaParts = [
		m.authors?.length
			? m.authors.length > 1
				? `${m.authors[0]} et al.`
				: m.authors[0]
			: null,
		m.year != null ? String(m.year) : null,
		m.venue || null,
	].filter(Boolean);
	const inLibrary = Boolean(matched.localMatch);
	const importable = !inLibrary && citationImportIdentifier(matched) != null;
	const link = citationExternalUrl(matched);

	const importIcon = importMenu?.importing ? (
		<Loader2 className="size-3.5 animate-spin" aria-hidden />
	) : (
		<Import className="size-3.5" aria-hidden />
	);

	return (
		<div
			role="dialog"
			aria-label={t("references.previewLabel", { marker: "" })}
			className="fixed z-50 w-[300px] rounded-xl border border-border/80 bg-background/98 p-3 shadow-xl ring-1 ring-black/5 backdrop-blur-sm dark:ring-white/10"
			style={{ left, top }}
			onPointerEnter={onPointerEnter}
			onPointerLeave={onPointerLeave}
		>
			<div className="flex items-center justify-between gap-2">
				{matched.display ? (
					<span className="shrink-0 font-medium text-[10px] text-muted-foreground tabular-nums">
						{matched.display}
					</span>
				) : (
					<span />
				)}
				<span className="flex items-center gap-1">
					{inLibrary ? (
						<BookCheck
							className="size-3.5 text-emerald-600 dark:text-emerald-500"
							aria-label={t("references.inLibrary")}
						/>
					) : importable && importMenu ? (
						<CitationImportPopover
							citationId={matched.id}
							folders={importMenu.folders}
							lastImportParentDir={importMenu.lastImportParentDir}
							importing={importMenu.importing}
							onImport={(folder) => importMenu.onImport(matched, folder)}
							onOpenChange={importMenu.onOpenChange}
						>
							<button
								type="button"
								className="rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground"
								aria-label={t("references.import")}
								onClick={(e) => e.stopPropagation()}
							>
								{importIcon}
							</button>
						</CitationImportPopover>
					) : (
						<Import
							className="size-3.5 text-muted-foreground"
							aria-label={t("references.import")}
						/>
					)}
					{link ? (
						<button
							type="button"
							className="rounded p-0.5 text-muted-foreground transition-colors hover:text-foreground"
							aria-label={t("references.openLink")}
							onClick={(e) => {
								e.stopPropagation();
								openExternalUrl(link);
							}}
						>
							<ArrowUpRight className="size-3.5" aria-hidden />
						</button>
					) : null}
				</span>
			</div>
			<p className="mt-1 line-clamp-2 text-[13px] leading-snug text-foreground">
				{m.title ?? matched.raw ?? matched.rawKey ?? matched.id}
			</p>
			{metaParts.length ? (
				<p className="mt-0.5 truncate text-[11px] text-muted-foreground">
					{metaParts.join(" · ")}
				</p>
			) : null}
		</div>
	);
}
