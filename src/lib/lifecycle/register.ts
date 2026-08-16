/**
 * Central lifecycle handler registration. The bus runs handlers serially in
 * registration order, so cross-handler ordering lives in this file.
 */

import { invokeApi } from "@/lib/core/ipc";
import { isTauri } from "@/lib/core/tauri";
import { lifecycle } from "@/lib/lifecycle";
import {
	refreshLibrary,
	scheduleLibraryRefresh,
} from "@/lib/paper/library-store";
import { seedVaultSkills } from "@/lib/vault/actions";
import { isRemoteVaultHandle } from "@/lib/vault/remote/remote-vault";
import {
	getVaultPath,
	refreshTree,
	scheduleTreeRefresh,
} from "@/lib/vault/store";
import { rebuildWikiAndNotify } from "@/lib/wiki/store";

/** Batch imports emit one `paper:imported` per paper; merge the rebuilds. */
let importWikiTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleImportWikiRebuild(): void {
	if (importWikiTimer) clearTimeout(importWikiTimer);
	importWikiTimer = setTimeout(() => {
		importWikiTimer = null;
		const vault = getVaultPath();
		if (!vault || isRemoteVaultHandle(vault)) return;
		void rebuildWikiAndNotify(vault);
	}, 300);
}

export function registerLifecycleHandlers(): () => void {
	const offs = [
		lifecycle.on("vault:opened", ({ vaultPath }) => {
			void refreshTree(vaultPath);
			void refreshLibrary();
			seedVaultSkills(vaultPath);
			if (isTauri()) {
				// T2 reconcile: backfill PAPER.md for catalog papers missing it. Fire
				// & forget; jobs are idempotent and throttled (ParseBody cap = 1).
				void invokeApi(
					"job_reconcile_vault",
					{ args: { vaultPath } },
					{ fallback: "vault reconcile failed" },
				).catch(() => undefined);
			}
		}),
		lifecycle.on("paper:imported", () => {
			scheduleTreeRefresh();
			scheduleImportWikiRebuild();
			scheduleLibraryRefresh();
		}),
	];
	return () => {
		for (const off of offs) off();
	};
}
