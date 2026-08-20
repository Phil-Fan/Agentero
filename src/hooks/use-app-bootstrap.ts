/**
 * App bootstrap effects: store seeding, theme / locale / uiScale application,
 * restored-vault validation, lifecycle handler registration + wire bridge
 * (per-vault side effects run via `vault:opened`), and the native
 * settings-window closed listener.
 */

import { useTheme } from "next-themes";
import { useEffect, useState } from "react";
import { useSettings, useVaultStore } from "@/hooks/use-app-stores";
import { useVaultOpenRequest } from "@/hooks/use-vault-open-request";
import i18n, { resolveLocale } from "@/i18n";
import { startActivityTracking } from "@/lib/activity";
import { isTauri } from "@/lib/core/tauri";
import { initLifecycleBridge, lifecycle } from "@/lib/lifecycle";
import { registerLifecycleHandlers } from "@/lib/lifecycle/register";
import { startJobCompletionRefresh } from "@/lib/paper/job-refresh";
import { refreshLibrary } from "@/lib/paper/library-store";
import { initJobCenterExecutors } from "@/lib/pdf/layout/enqueue-paper-layout";
import { applyDocumentChrome } from "@/lib/settings";
import { initSettingsStore } from "@/lib/settings/react-store";
import { setSettingsOpenState } from "@/lib/shell/ui-store";
import { validateRestoredVault } from "@/lib/vault/actions";
import { initVaultStore, setTree, setTreeLoading } from "@/lib/vault/store";
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

	// Register lifecycle handlers, then bridge Tauri wire events into the bus.
	useEffect(() => {
		const unregister = registerLifecycleHandlers();
		if (!isTauri()) return unregister;
		const dispose = initLifecycleBridge();
		return () => {
			dispose();
			unregister();
		};
	}, []);

	// Per-vault side effects hang off vault:opened (see lifecycle/register.ts).
	// Returning the scope release makes switch / close / unmount all tear down.
	useEffect(() => {
		if (!vaultPath) {
			setTree([]);
			setTreeLoading(false);
			void refreshLibrary();
			return;
		}
		return lifecycle.emitScoped("vault:opened", {
			vaultId: vaultPath,
			timestamp: Date.now(),
		});
	}, [vaultPath]);

	// Mirror the native settings window's lifecycle into the ui store.
	useEffect(() => {
		if (!isTauri()) return;
		let unlisten: (() => void) | undefined;
		void (async () => {
			const { listen } = await import("@tauri-apps/api/event");
			unlisten = await listen<{ kind: string }>("window:closed", (e) => {
				if (e.payload?.kind === "settings") setSettingsOpenState(false);
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

	// Start listening for renderer-executed JobCenter offers.
	useEffect(() => {
		if (!isTauri()) return;
		initJobCenterExecutors();
		startJobCompletionRefresh();
	}, []);

	// Bootstrap complete: stores seeded, restored vault validated (async).
	useEffect(() => {
		void lifecycle.emit("app:ready", { timestamp: Date.now() });
	}, []);
}
