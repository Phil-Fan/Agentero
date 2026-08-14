/**
 * Import a paper from a 广场 source into the vault.
 *
 * Each papers.cool row carries its upstream landing page (arxiv.org, AAAI OJS,
 * OpenReview, ACL Anthology). That URL is a valid identifier for the existing
 * 魔棒 pipeline: `extract_primary_identifier` classifies any http(s) input as a
 * URL and the Host routes it to the Translator's `/web` endpoint, so venue
 * papers — which have neither an arXiv id nor a DOI — resolve the same way.
 */

import { notifyError } from "@/lib/core/notify";
import { lookupSubmit } from "@/lib/paper/import-actions";

export type PlazaImportRequest = {
	/** Source-local row id, echoed back so the frame can settle that row. */
	id: string;
	/** Upstream landing page fed to the importer. */
	url: string;
	title: string | null;
};

/**
 * The import runs inside a background task that `lookupSubmit` does not await,
 * so a failure in there never reaches our `catch`. Settle regardless after this
 * long so the row cannot stay stuck pending; the tasks panel still reports the
 * real error. A retry after a spurious timeout is harmless — the Host dedupes by
 * arXiv id / DOI / normalized title.
 */
const SETTLE_TIMEOUT_MS = 120_000;

/**
 * Resolves once the import finishes: `true` when at least one paper landed.
 *
 * Does not open the imported paper — that would pull the user out of the
 * browsing panel, which is the wrong trade while importing several in a row.
 */
export async function importPlazaPaper(
	request: PlazaImportRequest,
): Promise<boolean> {
	if (!/^https?:\/\//i.test(request.url)) {
		notifyError("这条论文没有可用的来源链接");
		return false;
	}
	return new Promise<boolean>((resolve) => {
		let settled = false;
		const timer = setTimeout(() => settle(false), SETTLE_TIMEOUT_MS);
		function settle(ok: boolean) {
			if (settled) return;
			settled = true;
			clearTimeout(timer);
			resolve(ok);
		}
		void lookupSubmit([request.url], {
			openImported: false,
			onComplete: (result) => settle(result.imported.length > 0),
		}).catch((error) => {
			notifyError(error instanceof Error ? error.message : String(error));
			settle(false);
		});
	});
}
