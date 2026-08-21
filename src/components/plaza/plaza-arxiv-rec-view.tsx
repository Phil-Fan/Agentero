/**
 * Native 广场 panel: today's arXiv papers ranked against the Vault library.
 *
 * The Host caches its same-day run, so opening this panel renders the stored
 * result first and only recomputes when the categories change or on request.
 */

import {
	Check,
	Download,
	ExternalLink,
	Languages,
	Loader2,
	RefreshCw,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { notifyError } from "@/lib/core/notify";
import { openExternalUrl } from "@/lib/core/open-external";
import { cn } from "@/lib/core/utils";
import { lookupSubmit } from "@/lib/paper/import-actions";
import { ARXIV_FEED_CHIPS } from "@/lib/plaza/feeds";
import {
	isEmptyCorpusError,
	isNoCandidatesError,
	isNoEmbeddingError,
	type RecommendItem,
	recommendArxiv,
	recommendArxivLast,
} from "@/lib/recommend";
import { openSettingsWindow } from "@/lib/shell/settings-window";
import { runTranslate } from "@/lib/translate";
import { getVaultPath } from "@/lib/vault/store";

/** Empty-state reason, so the panel can offer the matching next action. */
type EmptyReason = "noEmbedding" | "emptyCorpus" | "noCandidates" | null;

export function PlazaArxivRecView({ className }: { className?: string }) {
	const { t } = useTranslation("sidebar");
	const [items, setItems] = useState<RecommendItem[]>([]);
	const [categories, setCategories] = useState<string[]>(() => [
		...ARXIV_FEED_CHIPS,
	]);
	const [computedAt, setComputedAt] = useState<string | null>(null);
	const [busy, setBusy] = useState(false);
	const [emptyReason, setEmptyReason] = useState<EmptyReason>(null);
	const [translations, setTranslations] = useState<Record<string, string>>({});
	const [translating, setTranslating] = useState(false);
	const loadedRef = useRef(false);

	const handleError = useCallback((error: unknown) => {
		if (isNoEmbeddingError(error)) {
			setEmptyReason("noEmbedding");
			return;
		}
		if (isEmptyCorpusError(error)) {
			setEmptyReason("emptyCorpus");
			return;
		}
		if (isNoCandidatesError(error)) {
			setEmptyReason("noCandidates");
			return;
		}
		notifyError(error instanceof Error ? error.message : String(error));
	}, []);

	const run = useCallback(
		async (nextCategories: string[]) => {
			const vaultPath = getVaultPath();
			if (!vaultPath) return;
			setBusy(true);
			setEmptyReason(null);
			try {
				const result = await recommendArxiv({
					vaultPath,
					categories: nextCategories,
					force: true,
				});
				setItems(result.items);
				setComputedAt(result.computedAt);
				setCategories(result.categories);
				if (result.items.length === 0) setEmptyReason("noCandidates");
			} catch (error) {
				handleError(error);
			} finally {
				setBusy(false);
			}
		},
		[handleError],
	);

	// Show the stored run immediately; the vault-open prewarm keeps it current.
	useEffect(() => {
		if (loadedRef.current) return;
		loadedRef.current = true;
		const vaultPath = getVaultPath();
		if (!vaultPath) return;
		void recommendArxivLast(vaultPath)
			.then((stored) => {
				if (!stored) return;
				setItems(stored.items);
				setComputedAt(stored.computedAt);
				if (stored.categories.length > 0) setCategories(stored.categories);
			})
			.catch(() => {
				/* no stored run yet — the user can refresh */
			});
	}, []);

	const toggleCategory = useCallback(
		(category: string) => {
			const next = categories.includes(category)
				? categories.filter((c) => c !== category)
				: [...categories, category];
			if (next.length === 0) return;
			setCategories(next);
			void run(next);
		},
		[categories, run],
	);

	const translateAll = useCallback(async () => {
		if (translating || items.length === 0) return;
		setTranslating(true);
		const results = await Promise.allSettled(
			items.map((item) =>
				runTranslate({
					text: item.abstract,
					context: { surface: "arxiv-rec", paperId: item.arxivId },
				}),
			),
		);
		const next: Record<string, string> = { ...translations };
		let failed = 0;
		items.forEach((item, i) => {
			const r = results[i];
			if (r.status === "fulfilled") {
				next[item.arxivId] = r.value;
			} else {
				failed += 1;
			}
		});
		setTranslations(next);
		setTranslating(false);
		if (failed > 0) {
			notifyError(
				failed === items.length
					? "Translation failed"
					: `Translation failed for ${failed} of ${items.length} papers`,
			);
		}
	}, [items, translating, translations]);

	return (
		<div className={cn("flex h-full min-h-0 flex-col", className)}>
			<div className="flex shrink-0 flex-wrap items-center gap-1.5 border-b px-2.5 py-2">
				{ARXIV_FEED_CHIPS.map((category) => {
					const active = categories.includes(category);
					return (
						<button
							key={category}
							type="button"
							disabled={busy}
							onClick={() => toggleCategory(category)}
							className={cn(
								"rounded-full border px-2 py-0.5 font-mono text-[11px] transition-colors",
								active
									? "border-primary/40 bg-primary/10 text-foreground"
									: "text-muted-foreground hover:bg-muted/60",
								busy && "opacity-60",
								"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
							)}
							aria-pressed={active}
						>
							{category}
						</button>
					);
				})}
				<span className="ml-auto flex items-center gap-1.5">
					{computedAt ? (
						<span className="text-muted-foreground text-[11px]">
							{new Date(computedAt).toLocaleString()}
						</span>
					) : null}
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								disabled={translating || busy || items.length === 0}
								aria-label={
									translating
										? t("plaza.arxivRec.translating")
										: t("plaza.arxivRec.translate")
								}
								onClick={() => void translateAll()}
							>
								{translating ? (
									<Loader2 className="size-3.5 animate-spin" aria-hidden />
								) : (
									<Languages className="size-3.5" aria-hidden />
								)}
							</Button>
						</TooltipTrigger>
						<TooltipContent>
							{translating
								? t("plaza.arxivRec.translating")
								: t("plaza.arxivRec.translate")}
						</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								disabled={busy}
								aria-label={t("plaza.arxivRec.refresh")}
								onClick={() => void run(categories)}
							>
								{busy ? (
									<Loader2 className="size-3.5 animate-spin" aria-hidden />
								) : (
									<RefreshCw className="size-3.5" aria-hidden />
								)}
							</Button>
						</TooltipTrigger>
						<TooltipContent>{t("plaza.arxivRec.refresh")}</TooltipContent>
					</Tooltip>
				</span>
			</div>

			<div className="agentero-scroll min-h-0 flex-1 overflow-y-auto p-2.5">
				{items.length === 0 ? (
					<EmptyState reason={emptyReason} busy={busy} />
				) : (
					<div className="grid gap-2">
						{items.map((item) => (
							<RecommendCard
								key={item.arxivId}
								item={item}
								translation={translations[item.arxivId]}
							/>
						))}
					</div>
				)}
			</div>
		</div>
	);
}

function EmptyState({ reason, busy }: { reason: EmptyReason; busy: boolean }) {
	const { t } = useTranslation("sidebar");
	if (busy) {
		return (
			<div className="flex items-center justify-center gap-2 py-10 text-muted-foreground text-xs">
				<Loader2 className="size-3.5 animate-spin" aria-hidden />
				{t("plaza.arxivRec.computing")}
			</div>
		);
	}
	return (
		<div className="flex flex-col items-center gap-2 py-10 text-center">
			<p className="max-w-sm text-muted-foreground text-xs leading-relaxed">
				{reason === "noEmbedding"
					? t("plaza.arxivRec.needsEmbedding")
					: reason === "emptyCorpus"
						? t("plaza.arxivRec.needsCorpus")
						: reason === "noCandidates"
							? t("plaza.arxivRec.noCandidates")
							: t("plaza.arxivRec.idle")}
			</p>
			{reason === "noEmbedding" ? (
				<Button
					type="button"
					variant="outline"
					size="sm"
					onClick={() => openSettingsWindow("agent")}
				>
					{t("plaza.arxivRec.openSettings")}
				</Button>
			) : null}
		</div>
	);
}

function RecommendCard({
	item,
	translation,
}: {
	item: RecommendItem;
	translation?: string;
}) {
	const { t } = useTranslation("sidebar");
	const [busy, setBusy] = useState(false);
	const [imported, setImported] = useState(false);

	const importPaper = useCallback(async () => {
		if (busy || imported) return;
		setBusy(true);
		try {
			await lookupSubmit([item.url], {
				onComplete: (result) => {
					const ok =
						result.imported.length > 0 ||
						result.skipped.some(
							(row) =>
								row.reason === "already_in_library" ||
								row.reason === "duplicate_in_batch",
						);
					if (ok) setImported(true);
					setBusy(false);
				},
			});
		} catch (error) {
			notifyError(error instanceof Error ? error.message : String(error));
			setBusy(false);
		}
	}, [busy, imported, item.url]);

	return (
		<div className="group relative rounded-lg border bg-background p-2.5 pr-16 transition-colors hover:border-foreground/20 hover:bg-muted/50">
			<div className="flex items-baseline gap-2">
				<span className="min-w-0 flex-1 font-medium text-sm leading-snug">
					{item.title}
				</span>
			</div>
			<p className="mt-1 text-muted-foreground text-xs leading-snug">
				{translation ?? item.abstract}
			</p>
			<div className="absolute top-2 right-2 flex items-center gap-0.5">
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-sm"
							aria-label={t("plaza.feeds.openOriginal")}
							onClick={() => openExternalUrl(item.url)}
						>
							<ExternalLink className="size-3.5" aria-hidden />
						</Button>
					</TooltipTrigger>
					<TooltipContent>{t("plaza.feeds.openOriginal")}</TooltipContent>
				</Tooltip>
				{imported ? (
					<Check
						className="size-3.5 text-muted-foreground"
						aria-label={t("plaza.feeds.imported")}
					/>
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
				)}
			</div>
		</div>
	);
}
