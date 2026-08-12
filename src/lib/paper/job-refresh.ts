/**
 * Refresh the library (debounced, quiet) when JobCenter jobs that mutate the
 * catalog or assets settle. The file watcher already refreshes the tree for
 * on-disk changes; catalog edits (e.g. `body_source` after a download or
 * `PAPER.md` parse) are watcher-ignored, so the library needs this nudge.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
	JobChangedSnapshot,
	JobKind,
	JobState,
} from "@/lib/core/job-center";
import { isTauri } from "@/lib/core/tauri";
import { scheduleLibraryRefresh } from "@/lib/paper/library-store";

const REFRESH_ON_KINDS: ReadonlySet<JobKind> = new Set([
	"downloadAssets",
	"parseBody",
]);

function isTerminalJobState(state: JobState): boolean {
	return (
		state === "succeeded" ||
		state === "failed" ||
		state === "cancelled" ||
		state === "skipped"
	);
}

let unlisten: UnlistenFn | null = null;

export function startJobCompletionRefresh(): void {
	if (!isTauri() || unlisten) return;
	void listen<{ job: JobChangedSnapshot }>("job:changed", (event) => {
		const job = event.payload.job;
		if (!REFRESH_ON_KINDS.has(job.kind)) return;
		if (!isTerminalJobState(job.state)) return;
		scheduleLibraryRefresh();
	}).then((u) => {
		unlisten = u;
	});
}
