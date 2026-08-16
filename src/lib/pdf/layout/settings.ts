/**
 * PDF layout-analysis backend selection (Settings → Layout).
 * Mirrors the translate BYOK provider-config pattern: the Host keeps the
 * real API key; the WebView only ever sees a `*` mask.
 */

/** Layout provider ids with remote credentials. */
export const LAYOUT_PROVIDER_IDS = ["paddle", "mineru"] as const;
export type LayoutProviderId = (typeof LAYOUT_PROVIDER_IDS)[number];

/**
 * - `local`: on-device PP-DocLayoutV3 (ONNX in the renderer).
 * - `paddle`: remote PP-StructureV3 async job API (`/api/v2/ocr/jobs`).
 * - `mineru`: remote MinerU batch extract API (`/api/v4/file-urls/batch`).
 */
export const LAYOUT_BACKENDS = ["local", ...LAYOUT_PROVIDER_IDS] as const;
export type LayoutBackend = (typeof LAYOUT_BACKENDS)[number];

export type LayoutProviderConfig = {
	apiKey: string;
};

export type LayoutSettings = {
	backend: LayoutBackend;
	providerConfigs: Partial<Record<LayoutProviderId, LayoutProviderConfig>>;
};

export const DEFAULT_LAYOUT_SETTINGS: LayoutSettings = {
	backend: "local",
	providerConfigs: {},
};

/** Fixed AI Studio PaddleOCR jobs endpoint (shown read-only in Settings). */
export const LAYOUT_PADDLE_JOBS_URL =
	"https://paddleocr.aistudio-app.com/api/v2/ocr/jobs";

/** Docs / console pages for obtaining keys (settings UI external link). */
export const LAYOUT_PROVIDER_DOCS_URLS: Record<LayoutProviderId, string> = {
	paddle: "https://aistudio.baidu.com/account/accessToken",
	mineru: "https://mineru.net/apiManage/token",
};

export function isLayoutBackend(value: unknown): value is LayoutBackend {
	return (
		typeof value === "string" &&
		(LAYOUT_BACKENDS as readonly string[]).includes(value)
	);
}

export function isLayoutProviderId(value: unknown): value is LayoutProviderId {
	return (
		typeof value === "string" &&
		(LAYOUT_PROVIDER_IDS as readonly string[]).includes(value)
	);
}

/** Same mask convention as translate BYOK keys (all `*`, same length). */
export function maskLayoutApiKey(key: string): string {
	const n = key.trim().length;
	return n === 0 ? "" : "*".repeat(n);
}

export function isLayoutApiKeyMask(key: string): boolean {
	const t = key.trim();
	return t.length > 0 && t.split("").every((c) => c === "*");
}
