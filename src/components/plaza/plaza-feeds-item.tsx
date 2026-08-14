/**
 * One feed timeline row: paper card (import) or short card (open original).
 */

import { Check, Download, ExternalLink, Loader2 } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { openExternalUrl } from "@/lib/core/open-external";
import { cn } from "@/lib/core/utils";
import {
	type FeedItem,
	feedsMarkImported,
	importFeedPaper,
} from "@/lib/plaza/feeds";

function formatWhen(iso: string | null, locale: string): string {
	if (!iso) return "";
	const date = new Date(iso);
	if (Number.isNaN(date.getTime())) return "";
	const diffSec = Math.round((date.getTime() - Date.now()) / 1000);
	const abs = Math.abs(diffSec);
	const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
	if (abs < 60) return rtf.format(diffSec, "second");
	if (abs < 3600) return rtf.format(Math.round(diffSec / 60), "minute");
	if (abs < 86400) return rtf.format(Math.round(diffSec / 3600), "hour");
	if (abs < 86400 * 7) return rtf.format(Math.round(diffSec / 86400), "day");
	return new Intl.DateTimeFormat(locale, {
		month: "short",
		day: "numeric",
	}).format(date);
}

export function PlazaFeedItemRow({
	item,
	onImported,
}: {
	item: FeedItem;
	onImported: (next: FeedItem) => void;
}) {
	const { t, i18n } = useTranslation("sidebar");
	const [busy, setBusy] = useState(false);
	const isPaper = Boolean(item.paperUrl);
	const imported = Boolean(item.importedAt);
	const when = formatWhen(item.publishedAt, i18n.language);

	const importPaper = useCallback(async () => {
		if (busy || imported || !item.paperUrl) return;
		setBusy(true);
		try {
			const ok = await importFeedPaper(item);
			if (ok) onImported(await feedsMarkImported(item.id));
		} finally {
			setBusy(false);
		}
	}, [busy, imported, item, onImported]);

	const openOriginal = useCallback(() => {
		const href = item.url ?? item.paperUrl;
		if (href) openExternalUrl(href);
	}, [item.paperUrl, item.url]);

	const primary = isPaper && !imported ? importPaper : openOriginal;

	return (
		<div
			className={cn(
				"group flex items-start gap-3 rounded-lg border bg-background p-2.5",
				"transition-colors hover:border-foreground/20 hover:bg-muted/50",
				busy && "pointer-events-none opacity-70",
			)}
		>
			<div className="min-w-0 flex-1">
				<button
					type="button"
					disabled={busy}
					onClick={() => void primary()}
					className={cn(
						"w-full truncate text-left font-medium text-sm",
						"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
					)}
				>
					{item.title}
				</button>
				<div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-muted-foreground text-xs">
					<span className="truncate">{item.subscriptionTitle}</span>
					{when ? (
						<>
							<span aria-hidden>·</span>
							<span className="shrink-0">{when}</span>
						</>
					) : null}
				</div>
				{item.summaryText ? (
					<p className="mt-1 line-clamp-3 text-muted-foreground text-xs leading-snug">
						{item.summaryText}
					</p>
				) : null}
			</div>
			<div className="flex shrink-0 items-center gap-0.5">
				{isPaper ? (
					imported ? (
						<span className="inline-flex items-center gap-0.5 text-muted-foreground text-xs">
							<Check className="size-3" aria-hidden />
							{t("plaza.feeds.imported")}
						</span>
					) : (
						<Tooltip>
							<TooltipTrigger asChild>
								<Button
									type="button"
									variant="ghost"
									size="icon-sm"
									disabled={busy}
									aria-label={t("plaza.feeds.import")}
									onClick={() => void importPaper()}
								>
									{busy ? (
										<Loader2 className="size-3.5 animate-spin" aria-hidden />
									) : (
										<Download className="size-3.5" aria-hidden />
									)}
								</Button>
							</TooltipTrigger>
							<TooltipContent>{t("plaza.feeds.import")}</TooltipContent>
						</Tooltip>
					)
				) : null}
				{item.url || item.paperUrl ? (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								aria-label={t("plaza.feeds.openOriginal")}
								onClick={openOriginal}
							>
								<ExternalLink className="size-3.5" aria-hidden />
							</Button>
						</TooltipTrigger>
						<TooltipContent>{t("plaza.feeds.openOriginal")}</TooltipContent>
					</Tooltip>
				) : null}
			</div>
		</div>
	);
}
