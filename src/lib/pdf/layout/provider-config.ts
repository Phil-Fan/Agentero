/**
 * Shared save/probe logic for the layout provider config UIs
 * (Settings → Layout pane and the onboarding layout step).
 */

import { isTauri } from "@/lib/core/tauri";
import {
	invokeLayoutRemoteProbe,
	tinyProbeJpegBase64,
} from "@/lib/pdf/layout/paddle";
import {
	isLayoutApiKeyMask,
	type LayoutBackend,
	type LayoutProviderConfig,
	type LayoutProviderId,
	type LayoutSettings,
	maskLayoutApiKey,
} from "@/lib/pdf/layout/settings";
import type { AppSettings } from "@/lib/settings";
import { saveSettingsAsync } from "@/lib/settings";

/**
 * Persist one provider config (plaintext key goes to the Host, which keeps
 * the secret) and return the masked layout snapshot for in-memory UI state.
 */
export async function persistLayoutProviderConfig(opts: {
	settings: AppSettings;
	provider: LayoutProviderId;
	config: LayoutProviderConfig;
	/** Also switch the active backend (onboarding commits the choice). */
	backend?: LayoutBackend;
}): Promise<{ displayLayout: LayoutSettings }> {
	const layout = opts.settings.layout;
	const savedLayout: LayoutSettings = {
		...layout,
		...(opts.backend ? { backend: opts.backend } : {}),
		providerConfigs: {
			...layout.providerConfigs,
			[opts.provider]: opts.config,
		},
	};
	try {
		await saveSettingsAsync({ ...opts.settings, layout: savedLayout });
	} catch {
		// Still mask the UI below.
	}
	const masked = isLayoutApiKeyMask(opts.config.apiKey)
		? opts.config.apiKey
		: maskLayoutApiKey(opts.config.apiKey);
	return {
		displayLayout: {
			...savedLayout,
			providerConfigs: {
				...savedLayout.providerConfigs,
				[opts.provider]: { ...opts.config, apiKey: masked },
			},
		},
	};
}

/**
 * One-shot connectivity probe through the Host. A masked key is resolved
 * from stored settings; the base URL always comes from stored settings.
 */
export async function probeLayoutProvider(
	provider: LayoutProviderId,
	apiKey: string,
): Promise<boolean> {
	if (!isTauri()) return false;
	const base64 = tinyProbeJpegBase64();
	if (!base64) return false;
	try {
		await invokeLayoutRemoteProbe({
			provider,
			imageBase64: base64,
			apiKey: apiKey.trim() || undefined,
		});
		return true;
	} catch {
		return false;
	}
}
