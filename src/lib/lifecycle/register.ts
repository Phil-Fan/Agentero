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
import { joinVaultPath } from "@/lib/vault/path";
import { isRemoteVaultHandle } from "@/lib/vault/remote/remote-vault";
import {
	getVaultPath,
	refreshTree,
	scheduleTreeRefresh,
} from "@/lib/vault/store";
import { rebuildWikiAndNotify } from "@/lib/wiki/store";

/** Batch imports emit one `paper:imported` per paper; merge the rebuilds. */
let importWikiTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleImportWikiRebuild(vault: string): void {
	if (importWikiTimer) clearTimeout(importWikiTimer);
	importWikiTimer = setTimeout(() => {
		importWikiTimer = null;
		if (isRemoteVaultHandle(vault) || getVaultPath() !== vault) return;
		void rebuildWikiAndNotify(vault);
	}, 300);
}

export function registerLifecycleHandlers(): () => void {
	const offs = [
		lifecycle.on("vault:opened", ({ vaultId }) => {
			void refreshTree(vaultId);
			void refreshLibrary();
			seedVaultSkills(vaultId);
			if (isTauri()) {
				// T2 reconcile: backfill PAPER.md for catalog papers missing it. Fire
				// & forget; jobs are idempotent and throttled (ParseBody cap = 1).
				void invokeApi(
					"job_reconcile_vault",
					{ args: { vaultPath: vaultId } },
					{ fallback: "vault reconcile failed" },
				).catch(() => undefined);
			}
		}),
		lifecycle.on("paper:imported", ({ vaultId, paperId }) => {
			// `app.emit` broadcasts to every window; only react to the active vault.
			if (vaultId !== getVaultPath()) return;
			// `paperId` is the folder basename, so point the targeted refresh at
			// `papers/` (re-listed eagerly by the Host, org subfolders included)
			// instead of rebuilding the whole tree, which would re-mark lazily
			// expanded folders as pending and cascade extra listings.
			scheduleTreeRefresh(
				paperId ? [joinVaultPath(vaultId, `papers/${paperId}`)] : undefined,
			);
			scheduleImportWikiRebuild(vaultId);
			scheduleLibraryRefresh();
		}),
	];
	return () => {
		if (importWikiTimer) {
			clearTimeout(importWikiTimer);
			importWikiTimer = null;
		}
		for (const off of offs) off();
	};
}
