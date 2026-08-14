/**
 * Cool Papers (papers.cool) note fetch.
 *
 * The Host resolves the paper (arXiv id, else exact title match), pulls its Kimi
 * analysis and appends it to `NOTES.md`. Here we surface progress and reseed the
 * open NOTES editor. Labels are intentionally not routed through i18n.
 */

import { enqueueBackgroundTask } from "@/lib/core/background-tasks";
import { invokeApi } from "@/lib/core/ipc";
import { notifyError, notifySuccess, notifyWarning } from "@/lib/core/notify";
import { arxivUrls } from "@/lib/paper/arxiv";
import { resolvePaperCatalogRel } from "@/lib/paper/library-actions";
import { notesPathForPaper } from "@/lib/paper/paths";
import type { PaperMetadata } from "@/lib/paper/types";
import { joinVaultPath, readVaultFile } from "@/lib/vault";
import { getVaultPath } from "@/lib/vault/store";
import { refreshTabNotes } from "@/lib/workspace/store";

export const COOL_PAPERS_ORIGIN = "https://papers.cool";

type CoolPapersNotes = {
	found: boolean;
	appended: boolean;
	branch: string | null;
	paperId: string | null;
	url: string | null;
	matchedBy: string | null;
};

/**
 * Append the papers.cool Kimi analysis for one paper to its NOTES.md.
 *
 * A first-time analysis is generated upstream on demand and can take up to a
 * minute, so the call runs as a background task.
 */
export async function fetchCoolPapersNotes(meta: PaperMetadata): Promise<void> {
	const vaultPath = getVaultPath();
	if (!vaultPath) return;
	const rel = await resolvePaperCatalogRel(meta);
	if (!rel) {
		notifyError("无法定位论文目录");
		return;
	}
	const arxivId = meta.arxiv_id ? (arxivUrls(meta.arxiv_id)?.id ?? null) : null;
	const title = meta.title?.trim() || null;
	if (!arxivId && !title) {
		notifyWarning("这篇论文缺少 arXiv 标识号和标题，无法在 Cool Papers 上定位");
		return;
	}

	try {
		const result = await enqueueBackgroundTask(
			{ kind: "parse", title: "获取 Cool Papers 笔记", detail: title ?? rel },
			() =>
				invokeApi<CoolPapersNotes>(
					"paper_coolpapers_notes",
					{ args: { vaultPath, path: rel, arxivId, title } },
					{ fallback: "获取 Cool Papers 笔记失败" },
				),
		);

		if (!result.found) {
			notifyWarning("Cool Papers 上没有找到这篇论文，未写入笔记");
			return;
		}
		if (!result.appended) {
			notifySuccess("NOTES.md 中已有该解析，未重复写入");
			return;
		}

		const paperDir = joinVaultPath(vaultPath, rel);
		try {
			const content = await readVaultFile(notesPathForPaper(paperDir));
			refreshTabNotes(paperDir, content);
		} catch {
			// Reseeding the open editor is best-effort; the file is already written.
		}
		notifySuccess("Cool Papers 解析已追加到 NOTES.md");
	} catch (e) {
		notifyError(e instanceof Error ? e.message : String(e));
	}
}
