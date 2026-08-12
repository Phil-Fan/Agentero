/**
 * Renderer-side executor registry for Rust JobCenter jobs.
 *
 * Rust emits `job:offer` when a renderer-executed job (e.g. layout analysis)
 * starts. This module routes offers to the matching frontend executor and
 * provides helpers to report progress / completion back via `job_report`.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import i18n from "@/i18n";
import {
	type BackgroundTaskKind,
	completeBackgroundTask,
	failBackgroundTask,
	registerBackgroundTaskCancellation,
	releaseBackgroundTaskCancellation,
	startBackgroundTask,
	updateBackgroundTask,
} from "@/lib/core/background-tasks";
import { invokeApi } from "@/lib/core/ipc";
import { logger } from "@/lib/core/logger";

export type JobState =
	| "queued"
	| "running"
	| "succeeded"
	| "failed"
	| "cancelled"
	| "skipped";

export type JobOfferPayload = {
	jobId: string;
	kind: JobKind;
	vaultPath: string;
	paperPath?: string | null;
	force: boolean;
};

export type JobKind =
	| "parseRefs"
	| "parseBody"
	| "layoutAnalyze"
	| "layoutTranslate"
	| "downloadAssets"
	| "pageCount"
	| "wikiReindex";

export type JobExecutor = (offer: JobOfferPayload) => Promise<void>;

const executors = new Map<JobKind, JobExecutor>();
let unlisten: UnlistenFn | null = null;

export function registerJobExecutor(
	kind: JobKind,
	executor: JobExecutor,
): void {
	executors.set(kind, executor);
}

export async function startJobCenterExecutorListener(): Promise<void> {
	if (unlisten) return;
	unlisten = await listen<{ job: JobOfferPayload }>("job:offer", (event) => {
		const offer = event.payload.job;
		const executor = executors.get(offer.kind);
		if (!executor) {
			logger.warn("no executor registered for job offer", {
				kind: offer.kind,
				jobId: offer.jobId,
			});
			return;
		}
		void executor(offer).catch((error) => {
			logger.error("job executor failed", {
				kind: offer.kind,
				jobId: offer.jobId,
				error: error instanceof Error ? error.message : String(error),
			});
		});
	});
}

export function stopJobCenterExecutorListener(): void {
	unlisten?.();
	unlisten = null;
}

export async function jobReport(args: {
	jobId: string;
	progress?: number | null;
	phase?: string | null;
	error?: string | null;
	state?: JobState | null;
}): Promise<void> {
	await invokeApi(
		"job_report",
		{
			args: {
				jobId: args.jobId,
				progress: args.progress ?? undefined,
				phase: args.phase ?? undefined,
				error: args.error ?? undefined,
				state: args.state ?? undefined,
			},
		},
		{ allowVoid: true },
	);
}

/** Snapshot shape of the `job:changed` event payload's `job` field. */
export type JobChangedSnapshot = {
	id: string;
	kind: JobKind;
	state: JobState;
	paperPath?: string | null;
	progress?: number | null;
	phase?: string | null;
	error?: string | null;
};

/**
 * Job kinds projected into the background-tasks panel (§7.6). Kinds absent
 * here (pageCount / wikiReindex) stay silent to avoid idle-lane noise.
 */
const PROJECTED_JOB_KINDS: Partial<Record<JobKind, BackgroundTaskKind>> = {
	layoutAnalyze: "layout",
	parseRefs: "parse",
	parseBody: "pdfParse",
	downloadAssets: "download",
};

function projectedTaskKind(kind: JobKind): BackgroundTaskKind | null {
	return PROJECTED_JOB_KINDS[kind] ?? null;
}

function jobPanelTitle(kind: JobKind): string {
	switch (kind) {
		case "parseRefs":
			return i18n.t("app:tasks.parseRefs");
		case "parseBody":
			return i18n.t("app:tasks.pdfParse");
		case "downloadAssets":
			return i18n.t("app:tasks.downloadPaper");
		default:
			return i18n.t("app:tasks.layoutAnalysis");
	}
}

let projectionUnlisten: UnlistenFn | null = null;
let projectionStarting = false;
const wiredJobCancels = new Set<string>();

/**
 * Single global `job:changed` → background-tasks-panel projection (§7.6).
 * Mirrors projected JobCenter jobs into the task store keyed by job id, and
 * routes panel cancellation to `job_cancel`.
 */
export function startJobTaskProjection(): void {
	if (projectionUnlisten || projectionStarting) return;
	projectionStarting = true;
	void listen<{ job: JobChangedSnapshot }>("job:changed", (event) => {
		projectJobToBackgroundTask(event.payload.job);
	})
		.then((unlisten) => {
			projectionUnlisten = unlisten;
		})
		.finally(() => {
			projectionStarting = false;
		});
}

function projectJobToBackgroundTask(job: JobChangedSnapshot): void {
	const taskKind = projectedTaskKind(job.kind);
	if (!taskKind) return;
	const title = jobPanelTitle(job.kind);
	const detail = job.paperPath ?? undefined;
	switch (job.state) {
		case "queued":
		case "running": {
			startBackgroundTask({
				id: job.id,
				kind: taskKind,
				title,
				detail,
				running: job.state === "running",
				progress: typeof job.progress === "number" ? job.progress : null,
			});
			wireJobCancellation(job.id);
			updateBackgroundTask(
				job.id,
				{
					status: job.state === "running" ? "running" : "queued",
					progress: typeof job.progress === "number" ? job.progress : null,
					...(job.phase ? { detail: job.phase } : {}),
				},
				{ absoluteProgress: true },
			);
			return;
		}
		case "succeeded":
		case "skipped":
			completeBackgroundTask(job.id, detail);
			releaseJobCancellation(job.id);
			return;
		case "failed":
			failBackgroundTask(job.id, job.error?.trim() || title);
			releaseJobCancellation(job.id);
			return;
		case "cancelled":
			updateBackgroundTask(job.id, { status: "cancelled" });
			releaseJobCancellation(job.id);
			return;
	}
}

function wireJobCancellation(jobId: string): void {
	if (wiredJobCancels.has(jobId)) return;
	wiredJobCancels.add(jobId);
	const signal = registerBackgroundTaskCancellation(jobId);
	signal.addEventListener(
		"abort",
		() => {
			void invokeApi<boolean>(
				"job_cancel",
				{ jobId },
				{ fallback: "job cancellation failed" },
			).catch((error) =>
				logger.warn("job cancellation failed", {
					jobId,
					error: error instanceof Error ? error.message : String(error),
				}),
			);
		},
		{ once: true },
	);
}

function releaseJobCancellation(jobId: string): void {
	if (!wiredJobCancels.has(jobId)) return;
	wiredJobCancels.delete(jobId);
	releaseBackgroundTaskCancellation(jobId);
}
