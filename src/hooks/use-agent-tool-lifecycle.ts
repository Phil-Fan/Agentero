import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	type CatalogEntry,
	type CatalogScanResponse,
	runToolLifecycle as runAgentToolLifecycle,
	type ToolLifecycleAction,
} from "@/lib/agent";
import { errorText } from "@/lib/core/error";
import { notifyError, notifySuccess } from "@/lib/core/notify";
import { isTauri } from "@/lib/core/tauri";
import { listenSafe } from "@/lib/core/tauri-events";

type LifecycleProgressEvent = {
	taskId: string;
	phase: string;
	progress: number | null;
};

export type LifecycleProgressState = {
	progress: number | null;
	detail: string;
};

export function useAgentToolLifecycle(opts: {
	scanOnce: () => Promise<CatalogScanResponse | null>;
	probeInstalled?: (scan: CatalogScanResponse, force: boolean) => Promise<void>;
	onError?: (message: string) => void;
}) {
	const { t } = useTranslation("settings");
	const { scanOnce, probeInstalled, onError } = opts;
	const [lifecycleBusyIds, setLifecycleBusyIds] = useState<
		Map<string, ToolLifecycleAction>
	>(() => new Map());
	const [lifecycleProgress, setLifecycleProgress] = useState<
		Record<string, LifecycleProgressState>
	>({});

	const patchLifecycleProgress = useCallback(
		(templateId: string, patch: Partial<LifecycleProgressState>) => {
			setLifecycleProgress((prev) => {
				const current = prev[templateId] ?? {
					progress: null,
					detail: t("agent.lifecycleInstalling"),
				};
				return {
					...prev,
					[templateId]: { ...current, ...patch },
				};
			});
		},
		[t],
	);

	const clearLifecycleProgress = useCallback((templateId: string) => {
		setLifecycleProgress((prev) => {
			if (!(templateId in prev)) return prev;
			const next = { ...prev };
			delete next[templateId];
			return next;
		});
	}, []);

	const lifecyclePhaseLabel = useCallback(
		(phase: string) => {
			if (phase === "agent-lifecycle-waiting") {
				return t("agent.lifecycleWaiting");
			}
			if (phase === "agent-lifecycle-uninstall") {
				return t("agent.lifecycleUninstalling");
			}
			return t("agent.lifecycleInstalling");
		},
		[t],
	);

	const runToolLifecycle = useCallback(
		async (
			entry: CatalogEntry,
			action: ToolLifecycleAction,
		): Promise<boolean> => {
			if (!isTauri()) return false;
			setLifecycleBusyIds((prev) => {
				const next = new Map(prev);
				next.set(entry.templateId, action);
				return next;
			});
			const taskId = `agent-lifecycle-${entry.templateId}-${Date.now().toString(36)}`;
			const stopProgress = listenSafe<LifecycleProgressEvent>(
				"agent-lifecycle:progress",
				(payload) => {
					if (payload.taskId !== taskId) return;
					patchLifecycleProgress(entry.templateId, {
						progress: payload.progress,
						detail: lifecyclePhaseLabel(payload.phase),
					});
				},
			);
			try {
				patchLifecycleProgress(entry.templateId, {
					progress: 5,
					detail: t(
						action === "uninstall"
							? "agent.lifecycleUninstalling"
							: "agent.lifecycleInstalling",
					),
				});
				await runAgentToolLifecycle(entry.templateId, action, taskId);
				patchLifecycleProgress(entry.templateId, {
					progress: 70,
					detail: t("agent.lifecycleScanning"),
				});
				const scan = await scanOnce();
				if (scan && probeInstalled) {
					patchLifecycleProgress(entry.templateId, {
						progress: 85,
						detail: t("agent.lifecycleProbing"),
					});
					await probeInstalled(scan, true);
				}
				notifySuccess(
					t(
						action === "update"
							? "agent.updateSuccess"
							: action === "uninstall"
								? "agent.uninstallSuccess"
								: "agent.installSuccess",
						{ name: entry.name },
					),
				);
				return true;
			} catch (e) {
				const message = errorText(e);
				onError?.(message);
				notifyError(message);
				return false;
			} finally {
				stopProgress();
				clearLifecycleProgress(entry.templateId);
				setLifecycleBusyIds((prev) => {
					const next = new Map(prev);
					next.delete(entry.templateId);
					return next;
				});
			}
		},
		[
			clearLifecycleProgress,
			lifecyclePhaseLabel,
			onError,
			patchLifecycleProgress,
			probeInstalled,
			scanOnce,
			t,
		],
	);

	return {
		lifecycleBusyIds,
		lifecycleProgress,
		runToolLifecycle,
	};
}
