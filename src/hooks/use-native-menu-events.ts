import { useEffect } from "react";
import { isTauri } from "@/lib/core/tauri";

type NativeMenuHandlers = {
	onSettings: () => void;
	onOpenVault: () => void;
	onCreateVault: () => void;
	onRefresh: () => void;
	onToggleSidebar: () => void;
	onSplitPane: () => void;
	onToggleChat: () => void;
	onCloseTabOrWindow: () => void;
};

/**
 * Subscribe to the desktop native menu bar events (Agentero → Settings…, File,
 * View). No-op outside the Tauri shell. `new_window` is handled natively in Rust.
 */
export function useNativeMenuEvents(handlers: NativeMenuHandlers): void {
	const {
		onSettings,
		onOpenVault,
		onCreateVault,
		onRefresh,
		onToggleSidebar,
		onSplitPane,
		onToggleChat,
		onCloseTabOrWindow,
	} = handlers;

	useEffect(() => {
		if (!isTauri()) return;

		let cancelled = false;
		let unsub: (() => void) | undefined;

		void (async () => {
			const { listen } = await import("@tauri-apps/api/event");
			if (cancelled) return;

			unsub = await listen<{ action: string }>("menu:invoked", (e) => {
				switch (e.payload?.action) {
					case "settings":
						onSettings();
						break;
					case "open_vault":
						onOpenVault();
						break;
					case "create_vault":
						onCreateVault();
						break;
					case "refresh_tree":
						onRefresh();
						break;
					case "toggle_sidebar":
						onToggleSidebar();
						break;
					case "split_pane":
						onSplitPane();
						break;
					case "toggle_chat":
						onToggleChat();
						break;
					// File → Close / ⌘W (macOS menu accelerator; keydown also
					// handles non-macOS)
					case "close_tab_or_window":
						onCloseTabOrWindow();
						break;
				}
			});
		})();

		return () => {
			cancelled = true;
			unsub?.();
		};
	}, [
		onSettings,
		onOpenVault,
		onCreateVault,
		onRefresh,
		onToggleSidebar,
		onSplitPane,
		onToggleChat,
		onCloseTabOrWindow,
	]);
}
