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
import { ButtonGroup } from "@/components/ui/button-group";
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
import { formatBytes } from "@/lib/core/background-tasks";
import { errorMessage, notifyError, notifySuccess } from "@/lib/core/notify";
import { isTauri } from "@/lib/core/tauri";
import { listenSafe } from "@/lib/core/tauri-events";
import { cn } from "@/lib/core/utils";
import {
	emptySyncConfig,
	SYNC_INTERVAL_CHOICES,
	SYNC_PROGRESS_EVENT,
	SYNC_STATE_EVENT,
	type SyncBackendConfig,
	type SyncProgressEvent,
	type SyncScopeSizes,
	type SyncStateEvent,
	type SyncStatus,
	syncConfigure,
	syncDisconnect,
	syncGetStatus,
	syncNow,
	syncScopeSizes,
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
	const [scopeSizes, setScopeSizes] = useState<SyncScopeSizes | null>(null);

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
		const offs = [
			listenSafe<SyncStateEvent>(SYNC_STATE_EVENT, (payload) => {
				if (payload.vaultPath !== localVault) return;
				setSyncing(payload.status === "syncing");
				if (payload.status !== "syncing") {
					setProgress(null);
					void refresh().catch(() => undefined);
				}
			}),
			listenSafe<SyncProgressEvent>(SYNC_PROGRESS_EVENT, (payload) => {
				if (payload.vaultPath === localVault) setProgress(payload);
			}),
		];
		return () => {
			for (const off of offs) off();
		};
	}, [localVault, refresh]);

	useEffect(() => {
		if (!localVault) return;
		let cancelled = false;
		syncScopeSizes(localVault)
			.then((sizes) => {
				if (!cancelled) setScopeSizes(sizes);
			})
			.catch(() => undefined);
		return () => {
			cancelled = true;
		};
	}, [localVault]);

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

	const scope = form.scope;
	const scopeAll = scope.pdf && scope.source && scope.attachments;
	const scopeNone = !scope.pdf && !scope.source && !scope.attachments;
	const setScopePreset = (all: boolean) =>
		patch({ scope: { pdf: all, source: all, attachments: all } });
	const patchScope = (key: keyof SyncBackendConfig["scope"], value: boolean) =>
		patch({ scope: { ...scope, [key]: value } });

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
	const configChanged = JSON.stringify(form) !== JSON.stringify(status?.config);
	const primaryAction =
		!status?.configured || configChanged ? (
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
		) : (
			<Button
				type="button"
				size="sm"
				onClick={() => void runSync()}
				disabled={syncing}
			>
				{syncing ? (
					<LoaderCircle data-icon="inline-start" className="animate-spin" />
				) : (
					<CloudUpload data-icon="inline-start" />
				)}
				{t("sync.syncNow")}
			</Button>
		);

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
				actions={primaryAction}
			/>

			{status?.configured && !form.conditionalWrites ? (
				<p className="mb-3 text-muted-foreground text-xs">
					{t("sync.noConditionalWrites")}
				</p>
			) : null}

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

			<SettingsGroup>
				<SettingsRow label={helpLabel(t("sync.scope"), t("sync.scopeHint"))}>
					<ButtonGroup>
						<Button
							type="button"
							size="sm"
							variant={scopeAll ? "secondary" : "outline"}
							aria-pressed={scopeAll}
							onClick={() => setScopePreset(true)}
						>
							{t("sync.scopeFull")}
						</Button>
						<Button
							type="button"
							size="sm"
							variant={scopeNone ? "secondary" : "outline"}
							aria-pressed={scopeNone}
							onClick={() => setScopePreset(false)}
						>
							{t("sync.scopeNotesOnly")}
						</Button>
					</ButtonGroup>
				</SettingsRow>
				<SettingsRow label={t("sync.scopePdf")} htmlFor="sync-scope-pdf">
					<span className="flex items-center gap-2">
						{scopeSizes ? (
							<span className="text-muted-foreground text-xs tabular-nums">
								{formatBytes(scopeSizes.pdf)}
							</span>
						) : null}
						<Switch
							id="sync-scope-pdf"
							checked={scope.pdf}
							onCheckedChange={(checked) => patchScope("pdf", checked)}
						/>
					</span>
				</SettingsRow>
				<SettingsRow label={t("sync.scopeSource")} htmlFor="sync-scope-source">
					<span className="flex items-center gap-2">
						{scopeSizes ? (
							<span className="text-muted-foreground text-xs tabular-nums">
								{formatBytes(scopeSizes.source)}
							</span>
						) : null}
						<Switch
							id="sync-scope-source"
							checked={scope.source}
							onCheckedChange={(checked) => patchScope("source", checked)}
						/>
					</span>
				</SettingsRow>
				<SettingsRow
					label={t("sync.scopeAttachments")}
					htmlFor="sync-scope-attachments"
				>
					<span className="flex items-center gap-2">
						{scopeSizes ? (
							<span className="text-muted-foreground text-xs tabular-nums">
								{formatBytes(scopeSizes.attachments)}
							</span>
						) : null}
						<Switch
							id="sync-scope-attachments"
							checked={scope.attachments}
							onCheckedChange={(checked) => patchScope("attachments", checked)}
						/>
					</span>
				</SettingsRow>
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
