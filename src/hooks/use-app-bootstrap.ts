/**
 * App bootstrap effects: store seeding, theme / locale / uiScale application,
 * restored-vault validation, per-vault side effects (tree, library, skills),
 * and the native settings-window closed listener.
 */

import { useTheme } from "next-themes";
import { useEffect, useState } from "react";
import { useSettings, useVaultStore } from "@/hooks/use-app-stores";
import { useVaultOpenRequest } from "@/hooks/use-vault-open-request";
import i18n, { resolveLocale } from "@/i18n";
import { startActivityTracking } from "@/lib/activity";
import { invokeApi } from "@/lib/core/ipc";
import { isTauri } from "@/lib/core/tauri";
import { initLifecycleBridge } from "@/lib/lifecycle";
import { startJobCompletionRefresh } from "@/lib/paper/job-refresh";
import { refreshLibrary } from "@/lib/paper/library-store";
import { initJobCenterExecutors } from "@/lib/pdf/layout/enqueue-paper-layout";
import { applyDocumentChrome } from "@/lib/settings";
import { initSettingsStore } from "@/lib/settings/react-store";
import { setSettingsOpenState } from "@/lib/shell/ui-store";
import { seedVaultSkills, validateRestoredVault } from "@/lib/vault/actions";
import {
	initVaultStore,
	refreshTree,
	setTree,
	setTreeLoading,
} from "@/lib/vault/store";
import { initWorkspaceStore } from "@/lib/workspace/store";

export function useAppBootstrap(): void {
	const { setTheme } = useTheme();
	// CLI / deep-link: agentero open <path> → vault:open-request
	useVaultOpenRequest();

	// Seed stores from persisted state on first render (after settings boot).
	useState(() => {
		initSettingsStore();
		initVaultStore();
		initWorkspaceStore();
		return null;
	});

	const theme = useSettings((s) => s.theme);
	const locale = useSettings((s) => s.locale);
	const uiScale = useSettings((s) => s.uiScale);
	const interfaceFontFamily = useSettings((s) => s.interfaceFontFamily);
	const monoFontFamily = useSettings((s) => s.monoFontFamily);
	const vaultPath = useVaultStore((s) => s.vaultPath);

	useEffect(() => {
		setTheme(theme);
	}, [theme, setTheme]);

	useEffect(() => {
		const resolved = resolveLocale(locale);
		void i18n.changeLanguage(resolved);
		if (typeof document !== "undefined") {
			document.documentElement.lang = resolved;
		}
		if (!isTauri()) return;
		void (async () => {
			try {
				const { invoke } = await import("@tauri-apps/api/core");
				await invoke("set_locale", { locale: resolved });
			} catch {
				// Native menu keeps its previous locale; non-fatal.
			}
		})();
	}, [locale]);

	useEffect(() => {
		// Scale + interface/mono fonts. macOS traffic lights stay build-time only.
		applyDocumentChrome({
			uiScale,
			interfaceFontFamily,
			monoFontFamily,
		});
	}, [uiScale, interfaceFontFamily, monoFontFamily]);

	useEffect(() => startActivityTracking(), []);

	// Validate the restored local Vault before restoring its tree and tabs.
	useEffect(() => {
		validateRestoredVault();
	}, []);

	// Per-vault side effects: tree reload, library rows, bundled-skill seeding.
	useEffect(() => {
		if (!vaultPath) {
			setTree([]);
			setTreeLoading(false);
			void refreshLibrary();
			return;
		}
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
	}, [vaultPath]);

	// Mirror the native settings window's lifecycle into the ui store.
	useEffect(() => {
		if (!isTauri()) return;
		let unlisten: (() => void) | undefined;
		void (async () => {
			const { listen } = await import("@tauri-apps/api/event");
			unlisten = await listen("settings_window_closed", () => {
				setSettingsOpenState(false);
			});
		})();
		return () => {
			unlisten?.();
		};
	}, []);

	// Feature singleton windows clear popped-out flags when closed.
	useEffect(() => {
		if (!isTauri()) return;
		let unbind: (() => void) | undefined;
		void import("@/lib/shell/feature-window").then(
			({ bindFeatureWindowClosedListener }) => {
				unbind = bindFeatureWindowClosedListener();
			},
		);
		return () => {
			unbind?.();
		};
	}, []);

	// Bridge Tauri wire lifecycle events into the frontend bus.
	useEffect(() => {
		if (!isTauri()) return;
		let dispose: (() => void) | undefined;
		void initLifecycleBridge().then((d) => {
			dispose = d;
		});
		return () => {
			dispose?.();
		};
	}, []);

	// Start listening for renderer-executed JobCenter offers.
	useEffect(() => {
		if (!isTauri()) return;
		initJobCenterExecutors();
		startJobCompletionRefresh();
	}, []);
}
