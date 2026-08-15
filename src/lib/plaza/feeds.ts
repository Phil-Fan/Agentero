/**
 * Plaza feed subscriptions — Host IPC + paper import.
 *
 * @see docs/development/plaza-feeds.md
 */

import { invokeApi } from "@/lib/core/ipc";
import { notifyError } from "@/lib/core/notify";
import { lookupSubmit } from "@/lib/paper/import-actions";

export type FeedSub = {
	id: string;
	url: string;
	title: string;
	addedAt: string;
	lastFetchedAt: string | null;
	lastError: string | null;
	itemCount: number;
	pinned: boolean;
	pinnedAt: string | null;
};

export type FeedItem = {
	id: string;
	subscriptionId: string;
	subscriptionTitle: string;
	title: string;
	url: string | null;
	publishedAt: string | null;
	summaryText: string;
	contentHtml: string | null;
	paperUrl: string | null;
	importedAt: string | null;
	bodyMarkdown: string | null;
};

export type FeedFilter = "all" | "paper" | "other";

export const ARXIV_FEED_CHIPS = [
	"cs.AI",
	"cs.CL",
	"cs.LG",
	"cs.CV",
	"stat.ML",
] as const;

export function arxivFeedUrl(cat: string): string {
	return `https://rss.arxiv.org/rss/${cat}`;
}

const META_PREFIXES = [
	"arxiv:",
	"announce type:",
	"comments:",
	"subjects:",
	"journal-ref:",
	"report-no:",
	"license:",
	"abstract:",
] as const;

/** Drop arXiv RSS headers so cards show the abstract, not the id / type. */
export function cleanFeedSummary(raw: string): string {
	let rest = raw.replace(/<[^>]+>/g, " ").trim();
	for (;;) {
		const lower = rest.toLowerCase();
		const hit = META_PREFIXES.find((prefix) => lower.startsWith(prefix));
		if (!hit) break;
		const after = rest.slice(hit.length).trimStart();
		if (hit === "abstract:") {
			rest = after;
			continue;
		}
		const skip = after.search(/\s/);
		rest = (skip === -1 ? "" : after.slice(skip)).trimStart();
	}
	return stripTrailingEllipsis(rest.replace(/\s+/g, " ").trim());
}

/** Drop RSS teaser tails (`[...]`, `…`) from cards and the detail body. */
export function stripTrailingEllipsis(text: string): string {
	let rest = text.trim();
	for (;;) {
		const lower = rest.toLowerCase();
		let next = rest;
		if (lower.endsWith("[...]")) next = rest.slice(0, -5);
		else if (rest.endsWith("[…]")) next = rest.slice(0, -"[…]".length);
		else if (lower.endsWith("[..]")) next = rest.slice(0, -4);
		else if (rest.endsWith("...")) next = rest.slice(0, -3);
		else if (rest.endsWith("…")) next = rest.slice(0, -1);
		else break;
		rest = next.trimEnd();
	}
	return rest;
}

/** One Markdown document: `# Title` plus body, for `MessageResponse`. */
export function feedDetailMarkdown(item: FeedItem): string {
	const raw = item.bodyMarkdown?.trim() || cleanFeedSummary(item.summaryText);
	const body = stripTrailingEllipsis(raw);
	const title = item.title.trim();
	if (!title) return body;
	const first = body.split("\n")[0]?.trim() ?? "";
	const firstText = first.replace(/^#+\s*/, "").trim();
	const rest = body.split("\n").slice(1).join("\n").trim();
	if (firstText.toLowerCase() === title.toLowerCase()) {
		return rest ? `# ${title}\n\n${rest}` : `# ${title}`;
	}
	return body ? `# ${title}\n\n${body}` : `# ${title}`;
}

const SETTLE_TIMEOUT_MS = 120_000;

export async function feedsList(): Promise<FeedSub[]> {
	const data = await invokeApi<{ subscriptions: FeedSub[] }>(
		"feeds_list",
		undefined,
		{ fallback: "feeds.listFailed" },
	);
	return data.subscriptions;
}

export async function feedsAdd(url: string, title?: string): Promise<FeedSub> {
	return invokeApi<FeedSub>(
		"feeds_add",
		{ args: { url, title } },
		{ fallback: "feeds.addFailed" },
	);
}

export async function feedsRemove(id: string): Promise<void> {
	await invokeApi(
		"feeds_remove",
		{ args: { id } },
		{ fallback: "feeds.removeFailed", allowVoid: true },
	);
}

export async function feedsRename(id: string, title: string): Promise<FeedSub> {
	return invokeApi<FeedSub>(
		"feeds_rename",
		{ args: { id, title } },
		{ fallback: "feeds.renameFailed" },
	);
}

export async function feedsRefresh(opts?: {
	id?: string;
	staleOnly?: boolean;
}): Promise<{ subscriptions: FeedSub[]; fetched: number; failed: number }> {
	return invokeApi(
		"feeds_refresh",
		{ args: { id: opts?.id, staleOnly: opts?.staleOnly ?? false } },
		{ fallback: "feeds.refreshFailed" },
	);
}

export async function feedsItems(opts?: {
	subscriptionId?: string;
	filter?: FeedFilter;
	limit?: number;
	beforePublishedAt?: string;
	beforeId?: string;
}): Promise<FeedItem[]> {
	const data = await invokeApi<{ items: FeedItem[] }>(
		"feeds_items",
		{
			args: {
				subscriptionId: opts?.subscriptionId,
				filter: opts?.filter ?? "all",
				limit: opts?.limit ?? 100,
				beforePublishedAt: opts?.beforePublishedAt,
				beforeId: opts?.beforeId,
			},
		},
		{ fallback: "feeds.itemsFailed" },
	);
	return data.items;
}

export async function feedsMarkImported(id: string): Promise<FeedItem> {
	return invokeApi<FeedItem>(
		"feeds_mark_imported",
		{ args: { id } },
		{ fallback: "feeds.markFailed" },
	);
}

export async function feedsSetPinned(
	id: string,
	pinned: boolean,
): Promise<FeedSub> {
	return invokeApi<FeedSub>(
		"feeds_set_pinned",
		{ args: { id, pinned } },
		{ fallback: "feeds.pinFailed" },
	);
}

export async function feedsResolveBody(id: string): Promise<FeedItem> {
	return invokeApi<FeedItem>(
		"feeds_resolve_body",
		{ args: { id } },
		{ fallback: "feeds.resolveFailed" },
	);
}

export function compareFeedSubs(a: FeedSub, b: FeedSub): number {
	if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
	if (a.pinned && b.pinned) {
		return (b.pinnedAt ?? "").localeCompare(a.pinnedAt ?? "");
	}
	return a.addedAt.localeCompare(b.addedAt);
}

/** Import via the magic wand. Resolves true when the paper is in the library. */
export function importFeedPaper(item: FeedItem): Promise<boolean> {
	const url = item.paperUrl?.trim();
	if (!url) return Promise.resolve(false);
	return new Promise<boolean>((resolve) => {
		let settled = false;
		const timer = setTimeout(() => settle(false), SETTLE_TIMEOUT_MS);
		function settle(ok: boolean) {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			resolve(ok);
		}
		void lookupSubmit([url], {
			openImported: false,
			onComplete: (result) => {
				const ok =
					result.imported.length > 0 ||
					result.skipped.some(
						(row) =>
							row.reason === "already_in_library" ||
							row.reason === "duplicate_in_batch",
					);
				settle(ok);
			},
		}).catch((error) => {
			notifyError(error instanceof Error ? error.message : String(error));
			settle(false);
		});
	});
}

const FEED_ERROR_KEYS = {
	invalid_url: "plaza.feeds.errors.invalid_url",
	duplicate: "plaza.feeds.errors.duplicate",
	not_found: "plaza.feeds.errors.not_found",
	empty_title: "plaza.feeds.errors.empty_title",
	empty: "plaza.feeds.errors.empty",
	no_feed: "plaza.feeds.errors.no_feed",
	too_large: "plaza.feeds.errors.too_large",
	parse: "plaza.feeds.errors.parse",
	http: "plaza.feeds.errors.http",
	fetch: "plaza.feeds.errors.fetch",
	body: "plaza.feeds.errors.body",
} as const;

export type FeedErrorKey =
	(typeof FEED_ERROR_KEYS)[keyof typeof FEED_ERROR_KEYS];

export function hostErrorKey(message: string): FeedErrorKey | null {
	const text = message.trim();
	if (!text.startsWith("feeds.")) return null;
	const code = text.slice("feeds.".length).split(":")[0];
	return code in FEED_ERROR_KEYS
		? FEED_ERROR_KEYS[code as keyof typeof FEED_ERROR_KEYS]
		: null;
}
