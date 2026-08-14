/**
 * Off-main-thread loader + cache for citation destination key maps.
 *
 * `buildCitationDestKeyMap` (pdf-lib) parses the entire PDF object tree with
 * no lazy loading — seconds of blocking on large papers. This module keeps the
 * viewer responsive by:
 *
 * 1. Running the parse in a dedicated entity-file worker (blob-URL workers can
 *    fail on Windows WebView2), transferring the buffer instead of cloning it.
 * 2. Reusing the PDF bytes the viewer tab already holds (EmbedPDF structured-
 *    clones them to its engine worker, so the tab buffer stays intact) — one
 *    memcpy instead of a second full disk read.
 * 3. Memoizing results per `pdfPath:byteLength`, so reopening the same paper
 *    never parses twice in a session.
 *
 * `@tauri-apps/plugin-fs` is unavailable inside workers, so file discovery and
 * the disk-read fallback stay on the main thread; only the CPU-bound parse
 * moves off it.
 */

import { logger } from "@/lib/core/logger";
import { findLocalPdfPath, localFileToArrayBuffer } from "@/lib/paper";
import {
	buildCitationDestKeyMap,
	type CitationDestKeyMap,
} from "@/lib/pdf/citation-dest-keys";
import type {
	CitationDestKeysRequest,
	CitationDestKeysResponse,
} from "@/lib/pdf/citation-dest-keys.worker";

/** Max cached PDFs; each entry is a small coord → BibTeX-key string map. */
const CACHE_LIMIT = 16;

/** `pdfPath:byteLength` → parsed map (promise, so concurrent loads dedupe). */
const cache = new Map<string, Promise<CitationDestKeyMap>>();

// ---- Worker transport ----

let worker: Worker | null = null;
/** Sticky: once the worker fails to spawn/load, parse on the main thread. */
let workerFailed = false;
let nextRequestId = 1;
const pending = new Map<
	number,
	{
		resolve: (map: CitationDestKeyMap) => void;
		reject: (error: Error) => void;
	}
>();

function ensureWorker(): Worker | null {
	if (workerFailed) return null;
	if (worker) return worker;
	if (typeof Worker === "undefined") {
		workerFailed = true;
		return null;
	}
	try {
		worker = new Worker(
			new URL("./citation-dest-keys.worker.ts", import.meta.url),
			{ type: "module" },
		);
	} catch (error) {
		logger.warn("citation dest map: worker spawn failed", {
			error: error instanceof Error ? error.message : String(error),
		});
		workerFailed = true;
		return null;
	}
	worker.onmessage = (event: MessageEvent<CitationDestKeysResponse>) => {
		const request = pending.get(event.data.id);
		if (!request) return;
		pending.delete(event.data.id);
		if (event.data.ok) request.resolve(new Map(event.data.entries));
		else request.reject(new Error(event.data.error));
	};
	worker.onerror = (event) => {
		// Worker module failed to load/run: fail in-flight requests and fall
		// back to main-thread parsing for future documents.
		const error = new Error(event.message || "citation worker error");
		logger.warn("citation dest map: worker error", { error: error.message });
		for (const request of pending.values()) request.reject(error);
		pending.clear();
		worker?.terminate();
		worker = null;
		workerFailed = true;
	};
	return worker;
}

/**
 * Parse `cite.*` named destinations without blocking the main thread. The
 * buffer is transferred (zero-copy) — the caller hands over ownership. Falls
 * back to a main-thread parse when workers are unavailable (tests, spawn
 * failure).
 */
function parseCitationDestKeys(
	bytes: ArrayBuffer,
): Promise<CitationDestKeyMap> {
	const w = ensureWorker();
	if (!w) return buildCitationDestKeyMap(bytes);
	const id = nextRequestId++;
	return new Promise<CitationDestKeyMap>((resolve, reject) => {
		pending.set(id, { resolve, reject });
		const request: CitationDestKeysRequest = { id, bytes };
		w.postMessage(request, [bytes]);
	});
}

/**
 * Cache wrapper: one parse per key, failures evicted so they can be retried.
 * Exported for tests (worker + Tauri fs are unavailable under vitest).
 */
export function getCitationDestKeyMapCached(
	key: string,
	source: "viewer-bytes" | "disk",
	parse: () => Promise<CitationDestKeyMap>,
): Promise<CitationDestKeyMap> {
	const hit = cache.get(key);
	if (hit) {
		logger.debug("citation dest map: cache hit", { key });
		return hit;
	}
	const started = performance.now();
	const promise = parse().then((map) => {
		logger.debug("citation dest map: built", {
			key,
			source,
			entries: map.size,
			duration_ms: Math.round(performance.now() - started),
		});
		return map;
	});
	cache.set(key, promise);
	// Bounded FIFO: maps are tiny, this only guards a pathological session.
	while (cache.size > CACHE_LIMIT) {
		const oldest = cache.keys().next().value;
		if (oldest === undefined) break;
		cache.delete(oldest);
	}
	promise.catch(() => cache.delete(key));
	return promise;
}

/** Test-only: reset the memoized maps. */
export function clearCitationDestKeyMapCache(): void {
	cache.clear();
}

export type LoadCitationDestKeyMapOptions = {
	/** Absolute paper folder of the open PDF. */
	paperAbsPath: string;
	/**
	 * PDF bytes the viewer already holds (`tab.pdfBytes`). When present (and
	 * not detached) they are copied instead of re-reading the whole file from
	 * disk; the copy is what gets transferred to the parser worker, so the
	 * viewer's buffer is never detached.
	 */
	viewerBytes?: ArrayBuffer | null;
};

/**
 * Resolve the paper's local PDF and build (or reuse) its citation destination
 * key map. Returns null when no local PDF exists. Rejections propagate so the
 * caller decides how loudly to fail.
 */
export async function loadCitationDestKeyMap({
	paperAbsPath,
	viewerBytes,
}: LoadCitationDestKeyMapOptions): Promise<CitationDestKeyMap | null> {
	const pdfPath = await findLocalPdfPath(paperAbsPath);
	if (!pdfPath) return null;

	const reusable =
		viewerBytes && viewerBytes.byteLength > 0 ? viewerBytes : null;
	if (reusable) {
		return getCitationDestKeyMapCached(
			`${pdfPath}:${reusable.byteLength}`,
			"viewer-bytes",
			// Copy before transferring so the worker cannot detach the viewer's
			// buffer (a memcpy beats a second full disk read).
			() => parseCitationDestKeys(reusable.slice(0)),
		);
	}

	const bytes = await localFileToArrayBuffer(pdfPath);
	if (!bytes) return null;
	return getCitationDestKeyMapCached(
		`${pdfPath}:${bytes.byteLength}`,
		"disk",
		// Freshly read, exclusively owned: transfer without copying.
		() => parseCitationDestKeys(bytes),
	);
}
