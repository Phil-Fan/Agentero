/**
 * Layout provider registry. Adding a provider means: extend the id union in
 * `settings.ts`, add one descriptor here, and register a Rust engine.
 */

import type { LayoutSidecarMode } from "@/lib/pdf/layout/io";
import type {
	LayoutBackend,
	LayoutProviderId,
	ParserBackend,
} from "@/lib/pdf/layout/settings";

export type LayoutProviderDescriptor = {
	id: LayoutProviderId | "local";
	kind: "local" | "remote";
	requiresApiKey: boolean;
	supportsBaseUrl: boolean;
	/** `source.mode` written to the layout sidecar for this provider's runs. */
	sidecarMode: LayoutSidecarMode;
};

export type RemoteLayoutProviderDescriptor = LayoutProviderDescriptor & {
	id: Exclude<LayoutBackend, "local">;
	kind: "remote";
};

/** Credential-card shape shared by the layout and body-parser provider UIs. */
export type ProviderCardDescriptor = {
	id: LayoutProviderId;
	requiresApiKey: boolean;
	supportsBaseUrl: boolean;
	requiresModel?: boolean;
};

export const LAYOUT_PROVIDERS: Record<LayoutBackend, LayoutProviderDescriptor> =
	{
		local: {
			id: "local",
			kind: "local",
			requiresApiKey: false,
			supportsBaseUrl: false,
			sidecarMode: "embedpdf-layout",
		},
		paddle: {
			id: "paddle",
			kind: "remote",
			requiresApiKey: true,
			supportsBaseUrl: false,
			sidecarMode: "paddle-layout",
		},
		mineru: {
			id: "mineru",
			kind: "remote",
			requiresApiKey: true,
			supportsBaseUrl: true,
			sidecarMode: "mineru-layout",
		},
	};

export function layoutProviderFor(
	backend: string,
): LayoutProviderDescriptor | null {
	return (
		(LAYOUT_PROVIDERS as Record<string, LayoutProviderDescriptor>)[backend] ??
		null
	);
}

/** PAPER.md body-parser provider cards (`local` needs no credentials). */
export const PARSER_PROVIDERS: Record<
	ParserBackend,
	ProviderCardDescriptor | null
> = {
	local: null,
	paddle: { id: "paddle", requiresApiKey: true, supportsBaseUrl: false },
	mineru: { id: "mineru", requiresApiKey: true, supportsBaseUrl: true },
	openaiCompatible: {
		id: "openaiCompatible",
		requiresApiKey: true,
		supportsBaseUrl: true,
		requiresModel: true,
	},
};

export function parserProviderFor(
	backend: string,
): ProviderCardDescriptor | null {
	return (
		(PARSER_PROVIDERS as Record<string, ProviderCardDescriptor | null>)[
			backend
		] ?? null
	);
}

export function isRemoteLayoutProvider(
	descriptor: LayoutProviderDescriptor,
): descriptor is RemoteLayoutProviderDescriptor {
	return descriptor.kind === "remote";
}
