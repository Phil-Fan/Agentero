/**
 * arXiv daily recommendation — Host IPC.
 *
 * Ranks today's arXiv papers against the Vault library (embedding similarity
 * weighted toward recently added papers). The Host reuses its stored same-day
 * run unless `force` is set, so calling this on vault open is cheap.
 */

import { errorText } from "@/lib/core/error";
import { invokeApi } from "@/lib/core/ipc";

/** Categories the Host falls back to when neither caller nor state has any. */
export const DEFAULT_REC_CATEGORIES = [
	"cs.AI",
	"cs.CL",
	"cs.LG",
	"cs.CV",
	"stat.ML",
] as const;

export type RecommendItem = {
	arxivId: string;
	title: string;
	abstract: string;
	url: string;
	publishedAt: string | null;
	score: number;
};

export type RecommendResult = {
	items: RecommendItem[];
	computedAt: string;
	categories: string[];
	corpusSize: number;
	/** True when the Host served its stored same-day run. */
	reusedCache: boolean;
};

/** Host marker for "no embedding endpoint configured" (Settings → Agent). */
export const ERR_NO_EMBEDDING = "recommend.no_embedding";
/** Host marker for "library has no abstracts to compare against". */
export const ERR_EMPTY_CORPUS = "recommend.empty_corpus";
/** Host marker for "the arXiv feeds returned nothing usable". */
export const ERR_NO_CANDIDATES = "recommend.no_candidates";

export function isNoEmbeddingError(error: unknown): boolean {
	return errorMessage(error) === ERR_NO_EMBEDDING;
}

export function isEmptyCorpusError(error: unknown): boolean {
	return errorMessage(error) === ERR_EMPTY_CORPUS;
}

export function isNoCandidatesError(error: unknown): boolean {
	return errorMessage(error) === ERR_NO_CANDIDATES;
}

function errorMessage(error: unknown): string {
	return errorText(error).trim();
}

export async function recommendArxiv(opts: {
	vaultPath: string;
	categories?: string[];
	topN?: number;
	force?: boolean;
}): Promise<RecommendResult> {
	return invokeApi<RecommendResult>(
		"recommend_arxiv",
		{
			args: {
				vaultPath: opts.vaultPath,
				categories: opts.categories,
				topN: opts.topN,
				force: opts.force ?? false,
			},
		},
		{ fallback: "recommend.failed" },
	);
}

/** Stored run, or null when this vault has never computed one. */
export async function recommendArxivLast(
	vaultPath: string,
): Promise<RecommendResult | null> {
	const result = await invokeApi<RecommendResult | null>(
		"recommend_arxiv_last",
		{ args: { vaultPath } },
		{ fallback: "recommend.failed", allowVoid: true },
	);
	return result ?? null;
}
