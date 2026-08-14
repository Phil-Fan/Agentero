/**
 * Dedicated worker: parse hyperref `cite.*` named destinations off the main
 * thread.
 *
 * pdf-lib has no lazy parsing — `PDFDocument.load` walks the whole PDF object
 * tree, which blocks for seconds on 100MB+ papers. The parse runs here and
 * only the small `pageIndex:pdfY → bibtexKey` entry list crosses back.
 *
 * Entity-file module worker on purpose: blob-URL workers can fail to spawn on
 * Windows WebView2, so this must stay a real file that Vite bundles as its own
 * worker chunk (`new URL(..., import.meta.url)` in `citation-dest-map.ts`).
 */

import { buildCitationDestKeyMap } from "@/lib/pdf/citation-dest-keys";

export type CitationDestKeysRequest = {
	id: number;
	/** PDF bytes; transferred in by the caller (zero-copy). */
	bytes: ArrayBuffer;
};

export type CitationDestKeysResponse =
	| { id: number; ok: true; entries: [string, string][] }
	| { id: number; ok: false; error: string };

/** Typed view of the dedicated-worker scope (tsconfig lib is DOM, not webworker). */
const scope = self as unknown as {
	onmessage: ((event: MessageEvent<CitationDestKeysRequest>) => void) | null;
	postMessage: (message: CitationDestKeysResponse) => void;
};

scope.onmessage = (event) => {
	const { id, bytes } = event.data;
	void buildCitationDestKeyMap(bytes)
		.then((map) => {
			scope.postMessage({ id, ok: true, entries: [...map.entries()] });
		})
		.catch((error: unknown) => {
			scope.postMessage({
				id,
				ok: false,
				error: error instanceof Error ? error.message : String(error),
			});
		});
};
