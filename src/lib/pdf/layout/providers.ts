/**
 * Layout provider registry. Adding a provider means: extend the id union in
 * `settings.ts`, add one descriptor here, and register a Rust engine.
 */

import type { LayoutSidecarMode } from "@/lib/pdf/layout/io";
import type {
	LayoutBackend,
	LayoutProviderId,
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
	id: LayoutProviderId;
	kind: "remote";
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

export function isRemoteLayoutProvider(
	descriptor: LayoutProviderDescriptor,
): descriptor is RemoteLayoutProviderDescriptor {
	return descriptor.kind === "remote";
}
