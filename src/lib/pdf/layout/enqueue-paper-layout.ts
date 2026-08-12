/**
 * Enqueue post-download / post-import layout analysis as a JobCenter task.
 * The renderer registers as the executor and runs the ONNX model when Rust
 * emits `job:offer` for a `layoutAnalyze` job.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import i18n from "@/i18n";
import { invokeApi } from "@/lib/core/ipc";
import {
	type JobOfferPayload,
	type JobState,
	jobReport,
	registerJobExecutor,
	startJobCenterExecutorListener,
	startJobTaskProjection,
} from "@/lib/core/job-center";
import { logger } from "@/lib/core/logger";
import { analyzePaperLayoutHeadless } from "@/lib/pdf/layout/headless-analyze";
import { readLayoutSidecar } from "@/lib/pdf/layout/io";
import { layoutAnalysisStore } from "@/lib/pdf/layout/store";
import { getVaultPath } from "@/lib/vault/store";

const JOB_CHANGED_EVENT = "job:changed";

const queuedPapers = new Set<string>();

function normalizePaperKey(paperAbsPath: string): string {
	return paperAbsPath.replace(/[/\\]+$/, "").replace(/\\/g, "/");
}

export function initJobCenterExecutors(): void {
	registerJobExecutor("layoutAnalyze", runLayoutAnalyzeExecutor);
	void startJobCenterExecutorListener();
	startJobTaskProjection();
}

async function runLayoutAnalyzeExecutor(offer: JobOfferPayload): Promise<void> {
	const paperAbsPath = offer.paperPath
		? `${offer.vaultPath}/${offer.paperPath}`.replace(/\\/g, "/")
		: offer.vaultPath;
	const paperLabel =
		offer.paperPath?.split("/").filter(Boolean).pop() || paperAbsPath;

	const abortController = new AbortController();
	let cancelledUnlisten: UnlistenFn | null = null;

	try {
		cancelledUnlisten = await listen<{ job: { id: string; state: JobState } }>(
			JOB_CHANGED_EVENT,
			(event) => {
				if (event.payload.job.id !== offer.jobId) return;
				if (event.payload.job.state === "cancelled") {
					abortController.abort();
				}
			},
		);

		const unsub = layoutAnalysisStore.subscribe((state) => {
			const { ui } = state;
			if (ui.stage !== "running" || typeof ui.progress !== "number") return;
			void jobReport({
				jobId: offer.jobId,
				progress: ui.progress,
				phase: ui.message?.trim() || i18n.t("viewer:figures.analyzing"),
			});
		});

		try {
			if (abortController.signal.aborted) throw new Error("cancelled");
			await analyzePaperLayoutHeadless({
				paperAbsPath,
				paperLabel,
				signal: abortController.signal,
			});
			await jobReport({
				jobId: offer.jobId,
				progress: 100,
				phase: "completed",
				state: "succeeded",
			});
		} catch (e) {
			const message = e instanceof Error ? e.message : String(e);
			const state = message.toLowerCase().includes("cancel")
				? "cancelled"
				: "failed";
			await jobReport({
				jobId: offer.jobId,
				state,
				error: state === "failed" ? message : undefined,
			});
		} finally {
			unsub();
		}
	} finally {
		cancelledUnlisten?.();
	}
}

/**
 * After assets land on disk, ensure layout.json is produced. Enqueues a
 * JobCenter `layoutAnalyze` job; the `job:changed` projection owns the task
 * panel row, progress, and cancellation (§7.4 入口②). No-op when the sidecar
 * already exists or a job is already queued for this paper this session.
 */
export function enqueuePaperLayoutAnalysis(opts: {
	paperAbsPath: string;
	paperLabel?: string;
}): void {
	const paperAbsPath = normalizePaperKey(opts.paperAbsPath);
	if (!paperAbsPath || queuedPapers.has(paperAbsPath)) return;

	const vaultPath = getVaultPath();
	if (!vaultPath) return;
	const paperRelPath = paperAbsPath
		.slice(vaultPath.length)
		.replace(/^[/\\]+/, "");
	if (!paperRelPath) return;

	queuedPapers.add(paperAbsPath);

	void (async () => {
		try {
			const cached = await readLayoutSidecar(paperAbsPath);
			if (cached?.regions?.length) return;
			await invokeApi(
				"job_layout_analyze_enqueue",
				{
					args: {
						vaultPath,
						path: paperRelPath,
						lane: "normal",
						force: false,
					},
				},
				{ fallback: "layout analysis enqueue failed" },
			);
		} catch (e) {
			logger.warn("enqueue paper layout analysis failed", {
				paperAbsPath,
				error: e instanceof Error ? e.message : String(e),
			});
		} finally {
			queuedPapers.delete(paperAbsPath);
		}
	})();
}
