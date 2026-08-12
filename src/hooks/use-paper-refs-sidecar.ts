import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { type CiteSidecar, loadPaperRefsReadOnly } from "@/lib/paper/refs";

/**
 * Read-only reference sidecar for a paper, reloaded when its ParseRefs
 * backfill job settles. Shared by the References panel and the PDF citation
 * hover preview (both need the same sidecar; JobCenter dedups the parse).
 */
export function usePaperRefsSidecar(
	vaultPath: string | null,
	paperPath: string | null,
): {
	sidecar: CiteSidecar | null;
	loading: boolean;
	setSidecar: (sidecar: CiteSidecar | null) => void;
} {
	const [sidecar, setSidecar] = useState<CiteSidecar | null>(null);
	const [loading, setLoading] = useState(false);

	useEffect(() => {
		setSidecar(null);
		if (!vaultPath || !paperPath) return;
		let cancelled = false;
		let unlisten: (() => void) | undefined;
		setLoading(true);
		const reload = () => {
			loadPaperRefsReadOnly(vaultPath, paperPath)
				.then((s) => {
					if (!cancelled) setSidecar(s);
				})
				.catch(() => {
					if (!cancelled) setSidecar(null);
				})
				.finally(() => {
					if (!cancelled) setLoading(false);
				});
		};
		reload();
		// Reload when this paper's ParseRefs backfill settles (event-driven,
		// replacing the old blocking list→parse fallback).
		const norm = (p: string | null | undefined) =>
			(p ?? "").replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
		void listen<{
			job: { kind: string; paperPath?: string | null; state: string };
		}>("job:changed", (event) => {
			const job = event.payload.job;
			if (job.kind !== "parseRefs") return;
			if (norm(job.paperPath) !== norm(paperPath)) return;
			if (
				job.state === "succeeded" ||
				job.state === "failed" ||
				job.state === "cancelled"
			) {
				if (!cancelled) reload();
			}
		}).then((u) => {
			if (cancelled) u();
			else unlisten = u;
		});
		return () => {
			cancelled = true;
			unlisten?.();
		};
	}, [vaultPath, paperPath]);

	return { sidecar, loading, setSidecar };
}
