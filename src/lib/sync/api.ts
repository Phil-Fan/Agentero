/**
 * Vault cloud sync (S3-compatible) — Host command wrappers + event names.
 * Design: docs/development/cloud-sync-s3.md
 */

import { invokeApi } from "@/lib/core/ipc";

export const SYNC_STATE_EVENT = "sync:state";
export const SYNC_PROGRESS_EVENT = "sync:progress";

export type SyncBackendConfig = {
	endpoint: string;
	region: string;
	bucket: string;
	prefix: string;
	accessKey: string;
	/** Masked (`***`) on the way out; send the mask back to keep the secret. */
	secretKey: string;
	forcePathStyle: boolean;
};

export type SyncStatus = {
	configured: boolean;
	config?: SyncBackendConfig;
	running: boolean;
	lastSyncAt?: string;
	lastVersion: number;
};

export type SyncOutcome = {
	version: number;
	uploaded: number;
	downloaded: number;
	deletedLocal: number;
	removedRemote: number;
	conflictCopies: string[];
};

export type SyncStateEvent = {
	vaultPath: string;
	status: "syncing" | "idle" | "error";
	error?: string;
};

export type SyncProgressEvent = {
	vaultPath: string;
	phase: "scan" | "pull" | "download" | "upload" | "finalize";
	current: number;
	total: number;
};

export const emptySyncConfig = (): SyncBackendConfig => ({
	endpoint: "",
	region: "us-east-1",
	bucket: "",
	prefix: "",
	accessKey: "",
	secretKey: "",
	forcePathStyle: true,
});

export function syncGetStatus(vaultPath: string): Promise<SyncStatus> {
	return invokeApi("sync_get_status", { args: { vaultPath } });
}

export function syncConfigure(
	vaultPath: string,
	config: SyncBackendConfig,
): Promise<SyncStatus> {
	return invokeApi("sync_configure", { args: { vaultPath, config } });
}

export function syncDisconnect(vaultPath: string): Promise<void> {
	return invokeApi(
		"sync_disconnect",
		{ args: { vaultPath } },
		{
			allowVoid: true,
		},
	);
}

export function syncNow(vaultPath: string): Promise<SyncOutcome> {
	return invokeApi("sync_now", { args: { vaultPath } });
}
