/**
 * Native 广场 panel: user RSS / Atom subscriptions and a paper-aware timeline.
 *
 * @see docs/development/plaza-feeds.md
 */

import { useVirtualizer } from "@tanstack/react-virtual";
import type { TFunction } from "i18next";
import { Plus, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { PlazaFeedItemRow } from "@/components/plaza/plaza-feeds-item";
import { Button } from "@/components/ui/button";
import {
	ContextMenu,
	ContextMenuContent,
	ContextMenuItem,
	ContextMenuSeparator,
	ContextMenuTrigger,
} from "@/components/ui/context-menu";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import {
	ResizableGroup,
	ResizableHandle,
	ResizablePanel,
} from "@/components/ui/resizable";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { copyTextToClipboard } from "@/lib/core/clipboard";
import { notifyError } from "@/lib/core/notify";
import { cn } from "@/lib/core/utils";
import {
	ARXIV_FEED_CHIPS,
	arxivFeedUrl,
	type FeedFilter,
	type FeedItem,
	type FeedSub,
	feedsAdd,
	feedsItems,
	feedsList,
	feedsRefresh,
	feedsRemove,
	feedsRename,
	hostErrorKey,
} from "@/lib/plaza/feeds";

const STALE_REFRESH_MS = 15 * 60 * 1000;

function toastHostError(message: string, t: TFunction<"sidebar">): void {
	const key = hostErrorKey(message);
	notifyError(key ? t(key) : message);
}

function formatHostError(message: string, t: TFunction<"sidebar">): string {
	const key = hostErrorKey(message);
	return key ? t(key) : message;
}

function AddForm({
	busy,
	onAdd,
}: {
	busy: boolean;
	onAdd: (url: string) => void;
}) {
	const { t } = useTranslation("sidebar");
	const [url, setUrl] = useState("");
	return (
		<form
			className="space-y-1.5"
			onSubmit={(event) => {
				event.preventDefault();
				const next = url.trim();
				if (!next || busy) return;
				onAdd(next);
				setUrl("");
			}}
		>
			<div className="flex gap-1">
				<Input
					value={url}
					onChange={(event) => setUrl(event.target.value)}
					placeholder={t("plaza.feeds.urlPlaceholder")}
					disabled={busy}
					aria-label={t("plaza.feeds.urlPlaceholder")}
					className="h-7 text-xs"
				/>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="submit"
							size="icon-sm"
							disabled={busy || !url.trim()}
							aria-label={t("plaza.feeds.add")}
						>
							<Plus className="size-3.5" aria-hidden />
						</Button>
					</TooltipTrigger>
					<TooltipContent>{t("plaza.feeds.add")}</TooltipContent>
				</Tooltip>
			</div>
			<div className="flex flex-wrap gap-1">
				{ARXIV_FEED_CHIPS.map((cat) => (
					<Button
						key={cat}
						type="button"
						variant="outline"
						size="xs"
						disabled={busy}
						onClick={() => setUrl(arxivFeedUrl(cat))}
					>
						{cat}
					</Button>
				))}
			</div>
		</form>
	);
}

export function PlazaFeedsView({ className }: { className?: string }) {
	const { t } = useTranslation("sidebar");
	const [subs, setSubs] = useState<FeedSub[]>([]);
	const [items, setItems] = useState<FeedItem[]>([]);
	const [selectedId, setSelectedId] = useState<string | null>(null);
	const [filter, setFilter] = useState<FeedFilter>("all");
	const [loading, setLoading] = useState(true);
	const [busy, setBusy] = useState(false);
	const [refreshing, setRefreshing] = useState(false);
	const [renameTarget, setRenameTarget] = useState<FeedSub | null>(null);
	const [renameTitle, setRenameTitle] = useState("");
	const listRef = useRef<HTMLDivElement>(null);

	const loadItems = useCallback(
		async (subscriptionId: string | null, nextFilter: FeedFilter) => {
			try {
				const rows = await feedsItems({
					subscriptionId: subscriptionId ?? undefined,
					filter: nextFilter,
				});
				setItems(rows);
			} catch (error) {
				toastHostError(
					error instanceof Error ? error.message : String(error),
					t,
				);
			}
		},
		[t],
	);

	const selectedIdRef = useRef(selectedId);
	const filterRef = useRef(filter);
	selectedIdRef.current = selectedId;
	filterRef.current = filter;

	useEffect(() => {
		let cancelled = false;
		void (async () => {
			setLoading(true);
			try {
				const next = await feedsList();
				if (cancelled) return;
				setSubs(next);
				const rows = await feedsItems({ filter: "all" });
				if (!cancelled) setItems(rows);
			} catch (error) {
				if (!cancelled) {
					toastHostError(
						error instanceof Error ? error.message : String(error),
						t,
					);
				}
			} finally {
				if (!cancelled) setLoading(false);
			}
		})();
		return () => {
			cancelled = true;
		};
	}, [t]);

	useEffect(() => {
		let cancelled = false;
		const runStale = async () => {
			try {
				const result = await feedsRefresh({ staleOnly: true });
				if (cancelled) return;
				setSubs(result.subscriptions);
				await loadItems(selectedIdRef.current, filterRef.current);
			} catch {
				/* stale refresh is best-effort */
			}
		};
		void runStale();
		const timer = window.setInterval(() => void runStale(), STALE_REFRESH_MS);
		return () => {
			cancelled = true;
			window.clearInterval(timer);
		};
	}, [loadItems]);

	const onAdd = useCallback(
		async (url: string) => {
			setBusy(true);
			try {
				const sub = await feedsAdd(url);
				setSubs((prev) =>
					prev.some((row) => row.id === sub.id) ? prev : [...prev, sub],
				);
				setSelectedId(sub.id);
				await loadItems(sub.id, filter);
			} catch (error) {
				toastHostError(
					error instanceof Error ? error.message : String(error),
					t,
				);
			} finally {
				setBusy(false);
			}
		},
		[filter, loadItems, t],
	);

	const onRefresh = useCallback(async () => {
		setRefreshing(true);
		try {
			const result = await feedsRefresh({
				id: selectedId ?? undefined,
			});
			setSubs(result.subscriptions);
			await loadItems(selectedId, filter);
		} catch (error) {
			toastHostError(error instanceof Error ? error.message : String(error), t);
		} finally {
			setRefreshing(false);
		}
	}, [filter, loadItems, selectedId, t]);

	const onRemove = useCallback(
		async (id: string) => {
			try {
				await feedsRemove(id);
				setSubs((prev) => prev.filter((row) => row.id !== id));
				if (selectedId === id) {
					setSelectedId(null);
					await loadItems(null, filter);
				} else {
					await loadItems(selectedId, filter);
				}
			} catch (error) {
				toastHostError(
					error instanceof Error ? error.message : String(error),
					t,
				);
			}
		},
		[filter, loadItems, selectedId, t],
	);

	const onRename = useCallback(async () => {
		if (!renameTarget) return;
		const title = renameTitle.trim();
		if (!title) return;
		try {
			const next = await feedsRename(renameTarget.id, title);
			setSubs((prev) => prev.map((row) => (row.id === next.id ? next : row)));
			setItems((prev) =>
				prev.map((row) =>
					row.subscriptionId === next.id
						? { ...row, subscriptionTitle: next.title }
						: row,
				),
			);
			setRenameTarget(null);
		} catch (error) {
			toastHostError(error instanceof Error ? error.message : String(error), t);
		}
	}, [renameTarget, renameTitle, t]);

	const virtualizer = useVirtualizer({
		count: items.length,
		getScrollElement: () => listRef.current,
		estimateSize: () => 92,
		overscan: 8,
	});

	const filters: FeedFilter[] = ["all", "paper", "other"];
	const selected = selectedId
		? (subs.find((row) => row.id === selectedId) ?? null)
		: null;
	const emptyItems = !loading && items.length === 0;

	return (
		<div className={cn("flex h-full min-h-0 flex-col", className)}>
			<ResizableGroup orientation="horizontal" className="min-h-0 flex-1">
				<ResizablePanel defaultSize={22} minSize={16} maxSize={40}>
					<div className="flex h-full min-h-0 flex-col gap-2 p-3">
						<AddForm busy={busy} onAdd={(url) => void onAdd(url)} />
						<div className="agentero-scroll min-h-0 flex-1 overflow-y-auto">
							<button
								type="button"
								onClick={() => {
									setSelectedId(null);
									void loadItems(null, filter);
								}}
								className={cn(
									"flex w-full rounded-md px-2 py-1.5 text-left text-sm",
									"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
									selectedId === null
										? "bg-muted text-foreground"
										: "text-muted-foreground hover:bg-muted/60 hover:text-foreground",
								)}
							>
								{t("plaza.feeds.all")}
							</button>
							{subs.map((sub) => (
								<ContextMenu key={sub.id}>
									<ContextMenuTrigger asChild>
										<button
											type="button"
											onClick={() => {
												setSelectedId(sub.id);
												void loadItems(sub.id, filter);
											}}
											className={cn(
												"mt-0.5 flex w-full flex-col rounded-md px-2 py-1.5 text-left",
												"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
												selectedId === sub.id
													? "bg-muted text-foreground"
													: "hover:bg-muted/60",
											)}
										>
											<span className="truncate text-sm">{sub.title}</span>
											{sub.lastError ? (
												<span className="truncate text-destructive text-xs">
													{formatHostError(sub.lastError, t)}
												</span>
											) : null}
										</button>
									</ContextMenuTrigger>
									<ContextMenuContent>
										<ContextMenuItem
											onSelect={() => {
												setRenameTarget(sub);
												setRenameTitle(sub.title);
											}}
										>
											{t("plaza.feeds.rename")}
										</ContextMenuItem>
										<ContextMenuItem
											onSelect={() =>
												void copyTextToClipboard(sub.url, {
													successMessage: t("plaza.feeds.copiedUrl"),
												})
											}
										>
											{t("plaza.feeds.copyUrl")}
										</ContextMenuItem>
										<ContextMenuItem
											onSelect={() =>
												void feedsRefresh({ id: sub.id }).then((result) => {
													setSubs(result.subscriptions);
													void loadItems(selectedId, filter);
												})
											}
										>
											{t("plaza.feeds.refreshOne")}
										</ContextMenuItem>
										<ContextMenuSeparator />
										<ContextMenuItem onSelect={() => void onRemove(sub.id)}>
											{t("plaza.feeds.remove")}
										</ContextMenuItem>
									</ContextMenuContent>
								</ContextMenu>
							))}
						</div>
					</div>
				</ResizablePanel>
				<ResizableHandle />
				<ResizablePanel defaultSize={78} minSize={40}>
					<div className="flex h-full min-h-0 flex-col">
						<div className="flex items-center gap-1 border-b px-3 py-2">
							{filters.map((key) => (
								<Button
									key={key}
									type="button"
									size="xs"
									variant={filter === key ? "secondary" : "ghost"}
									aria-pressed={filter === key}
									onClick={() => {
										setFilter(key);
										void loadItems(selectedId, key);
									}}
								>
									{t(`plaza.feeds.filter.${key}`)}
								</Button>
							))}
							<Tooltip>
								<TooltipTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="icon-sm"
										className="ml-auto"
										disabled={refreshing}
										aria-label={t("plaza.feeds.refresh")}
										onClick={() => void onRefresh()}
									>
										<RefreshCw
											className={cn("size-3.5", refreshing && "animate-spin")}
											aria-hidden
										/>
									</Button>
								</TooltipTrigger>
								<TooltipContent>{t("plaza.feeds.refresh")}</TooltipContent>
							</Tooltip>
						</div>
						{emptyItems ? (
							<div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center text-muted-foreground text-sm">
								{subs.length === 0 ? (
									<>
										<p>{t("plaza.feeds.emptyTitle")}</p>
										<p className="max-w-sm text-xs leading-relaxed">
											{t("plaza.feeds.emptyHint")}
										</p>
									</>
								) : (
									<p>
										{selected?.lastError
											? formatHostError(selected.lastError, t)
											: t("plaza.feeds.emptyItems")}
									</p>
								)}
							</div>
						) : (
							<div
								ref={listRef}
								className="agentero-scroll min-h-0 flex-1 overflow-y-auto px-3 py-2"
							>
								<div
									className="relative w-full"
									style={{ height: virtualizer.getTotalSize() }}
								>
									{virtualizer.getVirtualItems().map((virtualRow) => {
										const item = items[virtualRow.index];
										return (
											<div
												key={item.id}
												className="absolute top-0 left-0 w-full pb-2"
												style={{
													transform: `translateY(${virtualRow.start}px)`,
												}}
												ref={virtualizer.measureElement}
												data-index={virtualRow.index}
											>
												<PlazaFeedItemRow
													item={item}
													onImported={(next) =>
														setItems((prev) =>
															prev.map((row) =>
																row.id === next.id ? next : row,
															),
														)
													}
												/>
											</div>
										);
									})}
								</div>
							</div>
						)}
					</div>
				</ResizablePanel>
			</ResizableGroup>

			<Dialog
				open={renameTarget !== null}
				onOpenChange={(open) => {
					if (!open) setRenameTarget(null);
				}}
			>
				<DialogContent className="max-w-sm">
					<DialogHeader>
						<DialogTitle>{t("plaza.feeds.rename")}</DialogTitle>
					</DialogHeader>
					<Input
						value={renameTitle}
						onChange={(event) => setRenameTitle(event.target.value)}
						onKeyDown={(event) => {
							if (event.key === "Enter") {
								event.preventDefault();
								void onRename();
							}
						}}
						aria-label={t("plaza.feeds.rename")}
					/>
					<DialogFooter>
						<Button
							type="button"
							variant="ghost"
							onClick={() => setRenameTarget(null)}
						>
							{t("plaza.feeds.cancel")}
						</Button>
						<Button type="button" onClick={() => void onRename()}>
							{t("plaza.feeds.save")}
						</Button>
					</DialogFooter>
				</DialogContent>
			</Dialog>
		</div>
	);
}
