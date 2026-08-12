/**
 * Paper reference (citation) sidecar helpers.
 * Host parses references (online S2/Crossref → local bib/bbl fallback) into
 * the rebuildable `{paper}/source/agentero-cite.json`; see docs/backend/api.md
 * `paper_refs_parse` / `paper_refs_list`.
 */
import { invokeApi } from "@/lib/core/ipc";

export type CitationMeta = {
	title?: string;
	authors?: string[];
	year?: number;
	venue?: string;
	doi?: string;
	arxivId?: string;
	url?: string;
};

export type CitationLocalMatch = {
	/** Vault-relative path of the matched library paper. */
	paperPath: string;
	matchBy: "doi" | "arxiv" | "title";
};

export type Citation = {
	id: string;
	rawKey?: string;
	/** In-text marker like `[12]` when bibliography order is known. */
	display?: string;
	/** Raw bibliography entry text (always present for bbl/tex sources). */
	raw?: string;
	metadata: CitationMeta;
	localMatch?: CitationLocalMatch;
	/** e.g. `bbl`, `bib`, `s2`, `bbl+s2`. */
	source: string;
	status: "resolved" | "unresolved";
};

export type CiteSidecar = {
	schemaVersion: number;
	source: { mode: string; generatedAt: string; fingerprint: string };
	citations: Citation[];
	messages: string[];
};

/** Read the existing reference sidecar; `null` when not parsed yet. */
export async function paperRefsList(
	vaultPath: string,
	path: string,
): Promise<CiteSidecar | null> {
	const sidecar = await invokeApi<CiteSidecar | null>(
		"paper_refs_list",
		{ args: { vaultPath, path } },
		{ fallback: "paper_refs_list failed", allowVoid: true },
	);
	return sidecar ?? null;
}

/** Node type for the citation relationship graph (same shape as wiki graph). */
export type CiteGraphNodeType = "paper" | "note" | "index" | "stub";

export type CiteGraphNode = {
	id: string;
	label: string;
	type: CiteGraphNodeType;
	path?: string;
};

export type CiteGraphEdge = {
	id: string;
	source: string;
	target: string;
	targetRaw?: string;
};

export type CiteGraphResponse = {
	nodes: CiteGraphNode[];
	edges: CiteGraphEdge[];
	center?: string | null;
	depth: number;
};

/**
 * Citation relationship graph from reference sidecars + catalog localMatch.
 * Neighborhood: pass `center` (paper folder or file under it). Full library:
 * omit center. Does not parse missing sidecars.
 */
export async function paperRefsGraph(
	vaultPath: string | null,
	opts?: { center?: string | null; depth?: number | null },
): Promise<CiteGraphResponse> {
	if (!vaultPath) {
		return { nodes: [], edges: [], center: null, depth: opts?.depth ?? 1 };
	}
	return invokeApi<CiteGraphResponse>(
		"paper_refs_graph",
		{
			args: {
				vaultPath,
				center: opts?.center ?? null,
				depth: opts?.depth ?? null,
			},
		},
		{ fallback: "paper_refs_graph failed" },
	);
}

/** Parse (or force-refresh) references for one paper and persist the sidecar. */
export async function paperRefsParse(
	vaultPath: string,
	path: string,
	force = false,
): Promise<CiteSidecar> {
	return await invokeApi<CiteSidecar>(
		"paper_refs_parse",
		{ args: { vaultPath, path, force } },
		{ fallback: "paper_refs_parse failed" },
	);
}

/**
 * Load the reference sidecar for a paper (read-only). When it does not yet
 * exist, enqueue a JobCenter `ParseRefs` job to backfill it; the caller
 * reloads on `job:changed`. Replaces the old blocking list→parse fallback
 * (`loadPaperRefsAuto`), whose dedup is now the JobCenter's fingerprint key.
 */
export async function loadPaperRefsReadOnly(
	vaultPath: string,
	path: string,
): Promise<CiteSidecar | null> {
	const sidecar = await paperRefsList(vaultPath, path).catch(() => null);
	if (!sidecar) {
		void invokeApi(
			"job_parse_refs_enqueue",
			{ args: { vaultPath, path, force: false } },
			{ fallback: "refs parse enqueue failed" },
		).catch(() => undefined);
	}
	return sidecar;
}

/** Identifier usable by magic-wand import for an unmatched citation. */
export function citationImportIdentifier(citation: Citation): string | null {
	const { arxivId, doi } = citation.metadata;
	if (arxivId?.trim()) return `arXiv:${arxivId.trim()}`;
	if (doi?.trim()) return doi.trim();
	return null;
}
