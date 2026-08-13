/**
 * Cross-window onboarding request (Settings → main).
 * Settings emits a Tauri event; the main window forces the wizard open.
 */

import { isTauri } from "@/lib/core/tauri";

export const ONBOARDING_REQUEST_EVENT = "onboarding:request";

/** Settings (or any window): ask main to reopen the first-run wizard. */
export function broadcastOnboardingRequest(): void {
	if (!isTauri()) return;
	void (async () => {
		try {
			const { emit } = await import("@tauri-apps/api/event");
			await emit(ONBOARDING_REQUEST_EVENT);
		} catch {
			// non-fatal
		}
	})();
}

/** Main window only: resolve when the wizard should be force-opened. */
export async function listenOnboardingRequest(
	handler: () => void,
): Promise<() => void> {
	if (!isTauri()) return () => {};
	const { listen } = await import("@tauri-apps/api/event");
	return listen<unknown>(ONBOARDING_REQUEST_EVENT, () => {
		handler();
	});
}
