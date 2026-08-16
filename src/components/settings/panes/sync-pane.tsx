import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
	ChevronRight,
	CircleHelp,
	CloudUpload,
	LoaderCircle,
	Unplug,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	PageTitle,
	SettingsGroup,
	SettingsRow,
} from "@/components/settings/settings-layout";
import { Button } from "@/components/ui/button";
import {
	Collapsible,
	CollapsibleContent,
	CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { errorMessage, notifyError, notifySuccess } from "@/lib/core/notify";
import { isTauri } from "@/lib/core/tauri";
import { cn } from "@/lib/core/utils";
import {
	emptySyncConfig,
	SYNC_INTERVAL_CHOICES,
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

/** Label with a small "?" icon; the hint lives in a tooltip, not a row. */
function helpLabel(label: string, help: string) {
	return (
		<span className="inline-flex items-center gap-1">
			{label}
			<Tooltip>
				<TooltipTrigger asChild>
					<span className="cursor-help">
						<CircleHelp
							className="size-3 text-muted-foreground"
							aria-label={help}
						/>
					</span>
				</TooltipTrigger>
				<TooltipContent className="max-w-64">{help}</TooltipContent>
			</Tooltip>
		</span>
	);
}

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
	const [advancedOpen, setAdvancedOpen] = useState(false);

	const refresh = useCallback(async () => {
		if (!localVault) return;
		const next = await syncGetStatus(localVault);
		setStatus(next);
		setSyncing(next.running);
		if (next.config) {
			setForm(next.config);
			// Surface the advanced group when it holds non-default values.
			if (
				next.config.region !== "us-east-1" ||
				next.config.prefix !== "" ||
				!next.config.forcePathStyle
			) {
				setAdvancedOpen(true);
			}
		}
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

	const statusText =
		syncing && progress
			? t(`sync.phase.${progress.phase}`, {
					current: progress.current,
					total: progress.total,
				})
			: status?.lastSyncAt
				? t("sync.lastSync", {
						time: new Date(status.lastSyncAt).toLocaleString(),
					})
				: status?.configured
					? t("sync.neverSynced")
					: null;

	return (
		<section>
			<PageTitle
				title={
					<>
						{t("sync.title")}
						{statusText ? (
							<span className="ml-2 font-normal text-muted-foreground text-xs">
								{statusText}
							</span>
						) : null}
					</>
				}
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

			<SettingsGroup>
				<SettingsRow label={t("sync.endpoint")} htmlFor="sync-endpoint">
					{field("endpoint", { placeholder: "https://…" })}
				</SettingsRow>
				<SettingsRow label={t("sync.bucket")} htmlFor="sync-bucket">
					{field("bucket")}
				</SettingsRow>
				<SettingsRow label={t("sync.accessKey")} htmlFor="sync-accessKey">
					{field("accessKey")}
				</SettingsRow>
				<SettingsRow label={t("sync.secretKey")} htmlFor="sync-secretKey">
					{field("secretKey", { type: "password" })}
				</SettingsRow>
			</SettingsGroup>

			<SettingsGroup>
				<SettingsRow label={t("sync.autoSync")} htmlFor="sync-autoSync">
					<Switch
						id="sync-autoSync"
						checked={form.autoSync}
						onCheckedChange={(checked) => patch({ autoSync: checked })}
					/>
				</SettingsRow>
				{form.autoSync ? (
					<SettingsRow label={t("sync.interval")} htmlFor="sync-interval">
						<Select
							value={String(form.intervalMinutes)}
							onValueChange={(value) =>
								patch({ intervalMinutes: Number(value) })
							}
						>
							<SelectTrigger id="sync-interval" size="sm" className="w-32">
								<SelectValue />
							</SelectTrigger>
							<SelectContent>
								{SYNC_INTERVAL_CHOICES.map((minutes) => (
									<SelectItem key={minutes} value={String(minutes)}>
										{t("sync.intervalMinutes", { count: minutes })}
									</SelectItem>
								))}
							</SelectContent>
						</Select>
					</SettingsRow>
				) : null}
			</SettingsGroup>

			<Collapsible open={advancedOpen} onOpenChange={setAdvancedOpen}>
				<CollapsibleTrigger asChild>
					<button
						type="button"
						className="mb-2 flex items-center gap-1 text-muted-foreground text-xs outline-none transition-colors hover:text-foreground"
						aria-label={t("sync.advanced")}
					>
						<ChevronRight
							className={cn(
								"size-3.5 transition-transform",
								advancedOpen && "rotate-90",
							)}
						/>
						{t("sync.advanced")}
					</button>
				</CollapsibleTrigger>
				<CollapsibleContent>
					<SettingsGroup>
						<SettingsRow label={t("sync.region")} htmlFor="sync-region">
							{field("region")}
						</SettingsRow>
						<SettingsRow
							label={helpLabel(t("sync.prefix"), t("sync.prefixHint"))}
							htmlFor="sync-prefix"
						>
							{field("prefix")}
						</SettingsRow>
						<SettingsRow
							label={helpLabel(t("sync.pathStyle"), t("sync.pathStyleHint"))}
							htmlFor="sync-pathStyle"
						>
							<Switch
								id="sync-pathStyle"
								checked={form.forcePathStyle}
								onCheckedChange={(checked) =>
									patch({ forcePathStyle: checked })
								}
							/>
						</SettingsRow>
					</SettingsGroup>
				</CollapsibleContent>
			</Collapsible>

			<div className="flex items-center gap-2">
				{!status?.configured ||
				JSON.stringify(form) !== JSON.stringify(status.config) ? (
					<Button
						type="button"
						size="sm"
						onClick={() => void save()}
						disabled={saving || syncing}
					>
						{saving ? (
							<LoaderCircle data-icon="inline-start" className="animate-spin" />
						) : null}
						{status?.configured ? t("sync.save") : t("sync.connect")}
					</Button>
				) : null}
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
