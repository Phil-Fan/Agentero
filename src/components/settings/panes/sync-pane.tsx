import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { CloudUpload, LoaderCircle, Unplug } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	PageTitle,
	SettingsGroup,
	SettingsRow,
} from "@/components/settings/settings-layout";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { errorMessage, notifyError, notifySuccess } from "@/lib/core/notify";
import { isTauri } from "@/lib/core/tauri";
import {
	emptySyncConfig,
	SYNC_PROGRESS_EVENT,
	SYNC_STATE_EVENT,
	type SyncBackendConfig,
	type SyncProgressEvent,
	type SyncStateEvent,
	type SyncStatus,
	syncConfigure,
	syncDisconnect,
	syncGetStatus,
	syncNow,
} from "@/lib/sync/api";
import { isRemoteVaultHandle } from "@/lib/vault/remote/remote-vault";

export function SyncPane({ vaultPath }: { vaultPath: string | null }) {
	const { t } = useTranslation("settings");
	const localVault =
		isTauri() && vaultPath && !isRemoteVaultHandle(vaultPath)
			? vaultPath
			: null;

	const [status, setStatus] = useState<SyncStatus | null>(null);
	const [form, setForm] = useState<SyncBackendConfig>(emptySyncConfig());
	const [saving, setSaving] = useState(false);
	const [syncing, setSyncing] = useState(false);
	const [progress, setProgress] = useState<SyncProgressEvent | null>(null);

	const refresh = useCallback(async () => {
		if (!localVault) return;
		const next = await syncGetStatus(localVault);
		setStatus(next);
		setSyncing(next.running);
		if (next.config) setForm(next.config);
	}, [localVault]);

	useEffect(() => {
		void refresh().catch((error) => notifyError(errorMessage(error)));
	}, [refresh]);

	useEffect(() => {
		if (!localVault) return;
		const unlisten: UnlistenFn[] = [];
		void listen<SyncStateEvent>(SYNC_STATE_EVENT, ({ payload }) => {
			if (payload.vaultPath !== localVault) return;
			setSyncing(payload.status === "syncing");
			if (payload.status !== "syncing") {
				setProgress(null);
				void refresh().catch(() => undefined);
			}
		}).then((off) => unlisten.push(off));
		void listen<SyncProgressEvent>(SYNC_PROGRESS_EVENT, ({ payload }) => {
			if (payload.vaultPath === localVault) setProgress(payload);
		}).then((off) => unlisten.push(off));
		return () => {
			for (const off of unlisten) off();
		};
	}, [localVault, refresh]);

	if (!localVault) {
		return (
			<section>
				<PageTitle title={t("sync.title")} />
				<p className="text-muted-foreground text-sm">
					{t("sync.localVaultOnly")}
				</p>
			</section>
		);
	}

	const patch = (partial: Partial<SyncBackendConfig>) =>
		setForm((prev) => ({ ...prev, ...partial }));

	const save = async () => {
		setSaving(true);
		try {
			const next = await syncConfigure(localVault, form);
			setStatus(next);
			if (next.config) setForm(next.config);
			notifySuccess(t("sync.saved"));
		} catch (error) {
			notifyError(errorMessage(error));
		} finally {
			setSaving(false);
		}
	};

	const runSync = async () => {
		setSyncing(true);
		try {
			const outcome = await syncNow(localVault);
			notifySuccess(
				t("sync.done", {
					up: outcome.uploaded,
					down: outcome.downloaded,
				}),
			);
			if (outcome.conflictCopies.length > 0) {
				notifySuccess(
					t("sync.conflicts", { count: outcome.conflictCopies.length }),
				);
			}
		} catch (error) {
			notifyError(errorMessage(error));
		} finally {
			setSyncing(false);
			setProgress(null);
			void refresh().catch(() => undefined);
		}
	};

	const disconnect = async () => {
		try {
			await syncDisconnect(localVault);
			setStatus(null);
			setForm(emptySyncConfig());
			void refresh().catch(() => undefined);
		} catch (error) {
			notifyError(errorMessage(error));
		}
	};

	const field = (
		key: keyof SyncBackendConfig,
		opts?: { type?: string; placeholder?: string },
	) => (
		<Input
			id={`sync-${key}`}
			className="w-64"
			type={opts?.type ?? "text"}
			placeholder={opts?.placeholder}
			value={String(form[key])}
			onChange={(e) => patch({ [key]: e.target.value })}
			autoComplete="off"
			spellCheck={false}
		/>
	);

	return (
		<section>
			<PageTitle
				title={t("sync.title")}
				actions={
					status?.configured ? (
						<Button
							type="button"
							size="sm"
							onClick={() => void runSync()}
							disabled={syncing}
						>
							{syncing ? (
								<LoaderCircle
									data-icon="inline-start"
									className="animate-spin"
								/>
							) : (
								<CloudUpload data-icon="inline-start" />
							)}
							{t("sync.syncNow")}
						</Button>
					) : undefined
				}
			/>

			{status?.configured ? (
				<p className="mb-4 text-muted-foreground text-xs">
					{syncing && progress
						? t(`sync.phase.${progress.phase}`, {
								current: progress.current,
								total: progress.total,
							})
						: status.lastSyncAt
							? t("sync.lastSync", {
									time: new Date(status.lastSyncAt).toLocaleString(),
									version: status.lastVersion,
								})
							: t("sync.neverSynced")}
				</p>
			) : (
				<p className="mb-4 text-muted-foreground text-xs">{t("sync.intro")}</p>
			)}

			<SettingsGroup>
				<SettingsRow label={t("sync.endpoint")} htmlFor="sync-endpoint">
					{field("endpoint", { placeholder: "https://…" })}
				</SettingsRow>
				<SettingsRow label={t("sync.region")} htmlFor="sync-region">
					{field("region")}
				</SettingsRow>
				<SettingsRow label={t("sync.bucket")} htmlFor="sync-bucket">
					{field("bucket")}
				</SettingsRow>
				<SettingsRow
					label={t("sync.prefix")}
					htmlFor="sync-prefix"
					description={t("sync.prefixHint")}
				>
					{field("prefix")}
				</SettingsRow>
				<SettingsRow label={t("sync.accessKey")} htmlFor="sync-accessKey">
					{field("accessKey")}
				</SettingsRow>
				<SettingsRow label={t("sync.secretKey")} htmlFor="sync-secretKey">
					{field("secretKey", { type: "password" })}
				</SettingsRow>
				<SettingsRow
					label={t("sync.pathStyle")}
					htmlFor="sync-pathStyle"
					description={t("sync.pathStyleHint")}
				>
					<Switch
						id="sync-pathStyle"
						checked={form.forcePathStyle}
						onCheckedChange={(checked) => patch({ forcePathStyle: checked })}
					/>
				</SettingsRow>
			</SettingsGroup>

			<div className="flex items-center gap-2">
				<Button
					type="button"
					size="sm"
					variant={status?.configured ? "outline" : "default"}
					onClick={() => void save()}
					disabled={saving || syncing}
				>
					{saving ? (
						<LoaderCircle data-icon="inline-start" className="animate-spin" />
					) : null}
					{t("sync.saveAndTest")}
				</Button>
				{status?.configured ? (
					<Button
						type="button"
						size="sm"
						variant="ghost"
						onClick={() => void disconnect()}
						disabled={syncing}
					>
						<Unplug data-icon="inline-start" />
						{t("sync.disconnect")}
					</Button>
				) : null}
			</div>
		</section>
	);
}
