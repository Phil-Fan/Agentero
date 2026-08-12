import { useTranslation } from "react-i18next";
import type { ScreenPoint } from "@/components/viewer/pdf/types";
import type { Citation } from "@/lib/paper/refs";

const CARD_WIDTH = 300;
const CARD_ESTIMATED_HEIGHT = 210;

export function PdfCitationPreview({
	screen,
	previewText,
	matched,
	onPointerEnter,
	onPointerLeave,
}: {
	screen: ScreenPoint;
	previewText: string;
	matched?: Citation;
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
	const m = matched?.metadata;
	const metaParts = matched
		? [
				m?.authors?.length
					? m.authors.length > 1
						? `${m.authors[0]} et al.`
						: m.authors[0]
					: null,
				m?.year != null ? String(m.year) : null,
				m?.venue || null,
			].filter(Boolean)
		: [];

	return (
		<div
			role="dialog"
			aria-label={t("references.previewLabel", { marker: "" })}
			className="fixed z-50 w-[300px] rounded-xl border border-border/80 bg-background/98 p-3 shadow-xl ring-1 ring-black/5 backdrop-blur-sm dark:ring-white/10"
			style={{ left, top }}
			onPointerEnter={onPointerEnter}
			onPointerLeave={onPointerLeave}
		>
			<div className="flex items-baseline justify-between gap-2">
				<p className="font-medium text-[10px] uppercase tracking-wide text-muted-foreground">
					{t("references.previewExtracted")}
				</p>
				{matched?.rawKey ? (
					<span className="shrink-0 truncate font-mono text-[10px] text-muted-foreground">
						{matched.rawKey}
					</span>
				) : null}
			</div>
			<p className="mt-1 line-clamp-3 text-[13px] leading-snug text-foreground">
				{previewText}
			</p>
			{matched ? (
				<div className="mt-2 border-t border-border/60 pt-2">
					<p className="font-medium text-[10px] uppercase tracking-wide text-muted-foreground">
						{t("references.previewMatched")}
					</p>
					<div className="mt-1 flex items-baseline gap-1.5">
						{matched.display ? (
							<span className="shrink-0 font-medium text-[10px] text-muted-foreground tabular-nums">
								{matched.display}
							</span>
						) : null}
						<p className="line-clamp-2 text-[13px] leading-snug text-foreground">
							{m?.title ?? matched.raw ?? matched.rawKey ?? matched.id}
						</p>
					</div>
					{metaParts.length ? (
						<p className="mt-0.5 truncate text-[11px] text-muted-foreground">
							{metaParts.join(" · ")}
						</p>
					) : null}
				</div>
			) : null}
		</div>
	);
}
