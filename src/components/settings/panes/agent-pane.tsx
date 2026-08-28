import {
	ArrowUpCircle,
	ChevronDown,
	Loader2,
	Plus,
	RefreshCw,
	Terminal,
	Trash2,
	X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { AgentLogo } from "@/components/agent/agent-logo";
import { AgentCommonRows } from "@/components/settings/agent-common-rows";
import { AgentModelPicker } from "@/components/settings/agent-model-picker";
import {
	buildDefaultAgentChoices,
	catalogNeedsProbe,
	catalogProbeKey,
	catalogStatusTone,
	customProbeKey,
	type DefaultAgentChoice,
	defaultAgentChoiceValue,
	NO_DEFAULT_AGENT_CHOICE,
	ProbingBadge,
	patchCatalogProbe,
	patchCustomProbe,
	StatusBadge,
	showInstallAcp,
	showInstallAgent,
	showUninstallAgent,
	showUpdateAgent,
} from "@/components/settings/panes/agent-catalog";
import { AgentUninstallDialog } from "@/components/settings/panes/agent-uninstall-dialog";
import {
	PageTitle,
	SettingsGroup,
	SettingsRow,
} from "@/components/settings/settings-layout";
import type { SettingsHostContext } from "@/components/settings/types";
import { useProbingKeys } from "@/components/settings/use-probing-keys";
import { Button } from "@/components/ui/button";
import {
	DropdownMenu,
	DropdownMenuContent,
	DropdownMenuItem,
	DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Progress } from "@/components/ui/progress";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import { Textarea } from "@/components/ui/textarea";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { useAgentToolLifecycle } from "@/hooks/use-agent-tool-lifecycle";
import {
	type AgentTemplate,
	acpStatusLabel,
	type CatalogEntry,
	type CatalogScanResponse,
	ensureCatalogAgent,
	isAgentAuthFailure,
	listAgents,
	type ProbeResult,
	probeAgent,
	probeCatalogAgent,
	removeAgent,
	scanCatalog,
	setAgentUserAgent,
	setDefaultAgent,
	toolUninstallInfo,
	type UninstallInfo,
	USER_AGENT_PRESETS,
	upsertAgent,
} from "@/lib/agent";
import { errorText } from "@/lib/core/error";
import { notifyError, notifySuccess } from "@/lib/core/notify";
import { isTauri } from "@/lib/core/tauri";
import { cn } from "@/lib/core/utils";
import { probeEmbedding } from "@/lib/recommend";
import type { AppSettings, EmbeddingSettings } from "@/lib/settings";
import { isTranslateApiKeyMask } from "@/lib/translate";
import {
	remoteAgentOpenInstallTerminal,
	remoteAgentProbe,
	remoteAgentScan,
} from "@/lib/vault/remote/remote-vault";

type UninstallTarget =
	| { kind: "catalog"; entry: CatalogEntry; info: UninstallInfo | null }
	| { kind: "custom"; id: string; name: string; template: AgentTemplate };

export function AgentPane({
	settings,
	patch,
}: {
	settings: AppSettings;
	patch: (p: Partial<AppSettings>) => void;
}) {
	const { t } = useTranslation(["settings", "agent", "common"]);
	const [catalog, setCatalog] = useState<CatalogScanResponse | null>(null);
	const [loading, setLoading] = useState(false);
	const [savingDefaultValue, setSavingDefaultValue] = useState<string | null>(
		null,
	);
	const [adding, setAdding] = useState(false);
	/** Target of the uninstall/remove confirmation dialog. */
	const [uninstallTarget, setUninstallTarget] =
		useState<UninstallTarget | null>(null);
	const [uninstallBusy, setUninstallBusy] = useState(false);
	const [formName, setFormName] = useState(() => t("agent.form.defaultName"));
	const [formCommand, setFormCommand] = useState("");
	const [formArgs, setFormArgs] = useState("");
	/** Draft for optional ACP User-Agent (Codex / mid-station affinity). */
	const [userAgentDraft, setUserAgentDraft] = useState("");
	const [userAgentProviderDraft, setUserAgentProviderDraft] = useState("");
	// Embedding endpoint (BYOK) — local draft, committed on blur. apiKey may be a
	// host `*` mask on load; sending it back unchanged keeps the stored secret.
	const embedding = settings.embedding;
	const [embDraft, setEmbDraft] = useState<EmbeddingSettings>(() => ({
		...embedding,
	}));
	useEffect(() => {
		setEmbDraft({ ...embedding });
	}, [embedding]);
	const commitEmbedding = useCallback(
		(next: Partial<EmbeddingSettings>) => {
			patch({ embedding: { ...settings.embedding, ...next } });
		},
		[patch, settings.embedding],
	);
	const [embProbeBusy, setEmbProbeBusy] = useState(false);
	const [embProbeStatus, setEmbProbeStatus] = useState<
		"idle" | "probing" | "ok" | "failed" | "unconfigured"
	>("idle");
	// Auto-flip the dot to "unconfigured" when the draft lacks a baseUrl/model
	// (and we're not in the middle of a probe). Otherwise keep the last status.
	useEffect(() => {
		if (embProbeBusy) return;
		if (!embDraft.baseUrl.trim() || !embDraft.model.trim()) {
			setEmbProbeStatus("unconfigured");
		} else if (embProbeStatus === "unconfigured") {
			setEmbProbeStatus("idle");
		}
	}, [embDraft.baseUrl, embDraft.model, embProbeBusy, embProbeStatus]);
	const runEmbProbe = useCallback(async () => {
		const baseUrl = embDraft.baseUrl.trim();
		const model = embDraft.model.trim();
		if (!baseUrl || !model) return;
		setEmbProbeBusy(true);
		setEmbProbeStatus("probing");
		try {
			// Skip the apiKey override when the user has not typed a new key
			// (empty or Host `*` mask) so the Host falls back to the stored secret.
			const apiKey = embDraft.apiKey.trim();
			await probeEmbedding({
				baseUrl,
				apiKey: apiKey && !isTranslateApiKeyMask(apiKey) ? apiKey : undefined,
				model,
			});
			setEmbProbeStatus("ok");
		} catch {
			setEmbProbeStatus("failed");
		} finally {
			setEmbProbeBusy(false);
		}
	}, [embDraft.apiKey, embDraft.baseUrl, embDraft.model]);

	function embProbeDotClass(status: typeof embProbeStatus): string {
		switch (status) {
			case "ok":
				return "bg-emerald-500";
			case "failed":
				return "bg-destructive";
			case "probing":
				return "bg-amber-500 animate-pulse";
			case "unconfigured":
				return "bg-muted-foreground/35";
			default:
				return "bg-muted-foreground/50";
		}
	}
	const { probingKeys, setProbingKeys, clearProbingKey, clearAllProbingKeys } =
		useProbingKeys();
	const autoProbedRef = useRef(false);

	// PDF Ask agent/model (same listAgents registry as Translate → Agent)
	const [pdfAskRegistry, setPdfAskRegistry] = useState<Awaited<
		ReturnType<typeof listAgents>
	> | null>(null);
	const pdfAsk = settings.pdfAsk;
	const pdfAskValue = useMemo(
		() => ({ agentId: pdfAsk.agentId, modelId: pdfAsk.modelId }),
		[pdfAsk.agentId, pdfAsk.modelId],
	);
	const onPdfAskChange = useCallback(
		(next: { agentId: string; modelId: string }) => {
			patch({ pdfAsk: { ...settings.pdfAsk, ...next } });
		},
		[patch, settings.pdfAsk],
	);

	/** Scan only — does not toggle busy; callers own the loading flag. */
	const scanOnce =
		useCallback(async (): Promise<CatalogScanResponse | null> => {
			if (!isTauri()) {
				notifyError(t("agent.desktopOnly"));
				return null;
			}
			try {
				const scan = await scanCatalog();
				setCatalog(scan);
				setUserAgentDraft(scan.userAgent ?? "");
				setUserAgentProviderDraft(scan.userAgentProviderIds ?? "");
				return scan;
			} catch (e) {
				notifyError(errorText(e));
				return null;
			}
		}, [t]);

	const commitUserAgent = useCallback(
		async (override?: { userAgent?: string; providerIds?: string }) => {
			if (!isTauri()) return;
			const ua = (override?.userAgent ?? userAgentDraft).trim();
			const providers = (
				override?.providerIds ?? userAgentProviderDraft
			).trim();
			try {
				const next = await setAgentUserAgent(ua, providers);
				setUserAgentDraft(next.userAgent);
				setUserAgentProviderDraft(next.userAgentProviderIds);
				setCatalog((prev) =>
					prev
						? {
								...prev,
								userAgent: next.userAgent,
								userAgentProviderIds: next.userAgentProviderIds,
							}
						: prev,
				);
			} catch (e) {
				notifyError(errorText(e));
			}
		},
		[userAgentDraft, userAgentProviderDraft],
	);

	/**
	 * Parallel ACP probe. Soft open skips already-ready rows; force re-probes all
	 * installed. Badge updates from ProbeResult (no per-row full catalog rescan).
	 */
	const probeInstalled = useCallback(
		async (scan: CatalogScanResponse, force: boolean) => {
			if (!isTauri()) return;
			const candidates = scan.entries.filter((e) =>
				catalogNeedsProbe(e, force),
			);
			const custom = scan.customAgents.filter(
				(a) => a.available && (force || a.lastProbeOk !== true),
			);
			if (candidates.length === 0 && custom.length === 0) {
				clearAllProbingKeys();
				return;
			}

			setProbingKeys(
				new Set([
					...candidates.map((e) => catalogProbeKey(e.templateId)),
					...custom.map((a) => customProbeKey(a.id)),
				]),
			);

			await Promise.allSettled([
				...candidates.map(async (entry) => {
					const key = catalogProbeKey(entry.templateId);
					try {
						const result = await probeCatalogAgent(entry.templateId);
						setCatalog((prev) =>
							prev ? patchCatalogProbe(prev, entry.templateId, result) : prev,
						);
					} catch (e) {
						const err = errorText(e);
						setCatalog((prev) =>
							prev
								? patchCatalogProbe(prev, entry.templateId, {
										agentId: entry.registeredId ?? entry.templateId,
										available: false,
										error: err,
									})
								: prev,
						);
					} finally {
						clearProbingKey(key);
					}
				}),
				...custom.map(async (agent) => {
					const key = customProbeKey(agent.id);
					try {
						const result = await probeAgent(agent.id);
						setCatalog((prev) =>
							prev ? patchCustomProbe(prev, agent.id, result) : prev,
						);
					} catch (e) {
						const err = errorText(e);
						setCatalog((prev) =>
							prev
								? patchCustomProbe(prev, agent.id, {
										agentId: agent.id,
										available: false,
										error: err,
									})
								: prev,
						);
					} finally {
						clearProbingKey(key);
					}
				}),
			]);
		},
		[clearProbingKey, clearAllProbingKeys, setProbingKeys],
	);

	/**
	 * PATH scan → parallel probe → one reconcile scan.
	 * `force`: Refresh / proxy change re-probe everything; open page skips ready.
	 */
	const rescanAndProbe = useCallback(
		async (force = false) => {
			if (!isTauri()) {
				notifyError(t("agent.desktopOnly"));
				return;
			}
			setLoading(true);
			try {
				const scan = await scanOnce();
				// Release global busy after PATH scan so Install stays clickable while
				// ACP probes (often multi-second / timeouts) run in the background.
				setLoading(false);
				if (scan) {
					await probeInstalled(scan, force);
					await scanOnce();
				}
			} finally {
				setLoading(false);
				clearAllProbingKeys();
			}
		},
		[probeInstalled, scanOnce, t, clearAllProbingKeys],
	);

	// Open once: soft probe (skip ready). Refresh / proxy use force=true.
	useEffect(() => {
		if (autoProbedRef.current) return;
		autoProbedRef.current = true;
		void rescanAndProbe(false);
	}, [rescanAndProbe]);

	const refreshPdfAskRegistry = useCallback(async () => {
		if (!isTauri()) {
			setPdfAskRegistry(null);
			return;
		}
		try {
			setPdfAskRegistry(await listAgents());
		} catch {
			setPdfAskRegistry(null);
		}
	}, []);

	// Registry for PDF Ask agent/model selects (refresh when catalog changes)
	// biome-ignore lint/correctness/useExhaustiveDependencies: re-load after rescan/probe updates catalog
	useEffect(() => {
		void refreshPdfAskRegistry();
	}, [catalog, refreshPdfAskRegistry]);

	const onRescanAndProbe = async () => {
		await rescanAndProbe(true);
	};

	const defaultAgentChoices = useMemo(
		() => buildDefaultAgentChoices(catalog),
		[catalog],
	);
	const selectedDefaultValue = useMemo(
		() => defaultAgentChoiceValue(catalog, defaultAgentChoices),
		[catalog, defaultAgentChoices],
	);

	const onDefaultAgentChange = async (value: string) => {
		if (!isTauri() || value === NO_DEFAULT_AGENT_CHOICE) return;
		const choice = defaultAgentChoices.find((c) => c.value === value);
		if (!choice) return;
		setSavingDefaultValue(value);
		try {
			if (choice.source === "catalog" && choice.templateId) {
				await ensureCatalogAgent(choice.templateId, true);
			} else if (choice.agentId) {
				await setDefaultAgent(choice.agentId);
			}
			await scanOnce();
			await refreshPdfAskRegistry();
		} catch (e) {
			notifyError(errorText(e));
		} finally {
			setSavingDefaultValue(null);
		}
	};

	/** Silent install/update/uninstall: Host scopes Agent vs ACP from PATH (no free-form shell). */
	const {
		lifecycleBusyIds,
		lifecycleProgress,
		runToolLifecycle: onToolLifecycle,
	} = useAgentToolLifecycle({ scanOnce, probeInstalled });

	const openUninstallDialog = useCallback((target: UninstallTarget) => {
		if (!isTauri()) return;
		if (target.kind === "catalog") {
			setUninstallTarget({ ...target, info: null });
			void toolUninstallInfo(target.entry.templateId)
				.then((info) => {
					setUninstallTarget((prev) =>
						prev?.kind === "catalog" &&
						prev.entry.templateId === target.entry.templateId
							? { ...prev, info }
							: prev,
					);
				})
				.catch((e) => {
					notifyError(errorText(e));
					setUninstallTarget(null);
				});
			return;
		}
		setUninstallTarget(target);
	}, []);

	const onUninstallConfirm = async () => {
		const target = uninstallTarget;
		if (!target || !isTauri()) return;
		if (target.kind === "catalog") {
			const info = target.info;
			const hasPayload =
				info !== null && (info.npmCommands.length > 0 || info.dirs.length > 0);
			if (hasPayload) {
				// Full uninstall runs the lifecycle (binaries + registry entry).
				const entry = target.entry;
				setUninstallTarget(null);
				await onToolLifecycle(entry, "uninstall");
				return;
			}
			// Registry-only removal (e.g. hermes has no managed uninstall).
			setUninstallBusy(true);
			try {
				if (target.entry.registeredId) {
					await removeAgent(target.entry.registeredId);
				}
				await scanOnce();
				notifySuccess(t("agent.removeSuccess", { name: target.entry.name }));
			} catch (e) {
				notifyError(errorText(e));
			} finally {
				setUninstallBusy(false);
				setUninstallTarget(null);
			}
			return;
		}
		setUninstallBusy(true);
		try {
			await removeAgent(target.id);
			await scanOnce();
			notifySuccess(t("agent.removeSuccess", { name: target.name }));
		} catch (e) {
			notifyError(errorText(e));
		} finally {
			setUninstallBusy(false);
			setUninstallTarget(null);
		}
	};

	const onAddCustom = async () => {
		if (!isTauri()) return;
		setLoading(true);
		try {
			const args = formArgs.trim().split(/\s+/).filter(Boolean);
			await upsertAgent({
				name: formName.trim() || formCommand,
				template: "custom" as AgentTemplate,
				command: formCommand.trim(),
				args,
				setDefault: true,
			});
			setAdding(false);
			setFormCommand("");
			setFormArgs("");
			const scan = await scanOnce();
			if (scan) {
				await probeInstalled(scan, true);
				await scanOnce();
			}
		} catch (e) {
			notifyError(errorText(e));
		} finally {
			setLoading(false);
			clearAllProbingKeys();
		}
	};

	const entries = catalog?.entries ?? [];
	const customAgents = catalog?.customAgents ?? [];
	const busy = loading;

	return (
		<>
			<PageTitle title={t("agent.title")} />
			<SettingsGroup>
				<AgentCommonRows settings={settings} patch={patch} />
			</SettingsGroup>

			<p className="mb-1.5 font-medium text-muted-foreground text-xs uppercase tracking-wide">
				{t("agent.defaultAgent.section")}
			</p>
			<SettingsGroup>
				<SettingsRow
					label={t("agent.defaultAgent.label")}
					description={t("agent.defaultAgent.description")}
				>
					<Select
						value={selectedDefaultValue}
						onValueChange={(v) => void onDefaultAgentChange(v)}
						disabled={
							!isTauri() ||
							defaultAgentChoices.length === 0 ||
							Boolean(savingDefaultValue)
						}
					>
						<SelectTrigger size="sm" className="min-w-[220px] max-w-[300px]">
							<SelectValue placeholder={t("agent.defaultAgent.empty")} />
						</SelectTrigger>
						<SelectContent>
							{selectedDefaultValue === NO_DEFAULT_AGENT_CHOICE ? (
								<SelectItem value={NO_DEFAULT_AGENT_CHOICE} disabled>
									{defaultAgentChoices.length === 0
										? t("agent.defaultAgent.empty")
										: t("agent.defaultAgent.placeholder")}
								</SelectItem>
							) : null}
							{defaultAgentChoices.map((choice) => (
								<SelectItem key={choice.value} value={choice.value}>
									<AgentChoiceLabel choice={choice} />
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</SettingsRow>
			</SettingsGroup>

			{!isTauri() ? (
				<p className="mb-3 text-muted-foreground text-xs">
					{t("agent.desktopHint")}
				</p>
			) : null}

			{/* Common agents first — install/update before prefs that pick among them. */}
			<div className="mb-2 flex items-center justify-between gap-2">
				<p className="font-medium text-muted-foreground text-xs uppercase tracking-wide">
					{t("agent.commonAgents")}
				</p>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					aria-label={t("agent.probe")}
					title={t("agent.probe")}
					disabled={busy || !isTauri()}
					onClick={() => void onRescanAndProbe()}
				>
					{/* Loader2 while busy — avoid RefreshCw+spin (looks like two arrows, one stuck). */}
					{busy ? (
						<Loader2 className="size-3.5 animate-spin" aria-hidden />
					) : (
						<RefreshCw className="size-3.5" aria-hidden />
					)}
				</Button>
			</div>

			<SettingsGroup>
				{entries.length === 0 && busy ? (
					<div className="flex items-center gap-2 px-3.5 py-4 text-muted-foreground text-xs">
						<Loader2 className="size-3.5 animate-spin" aria-hidden />
						{t("agent.scanning")}
					</div>
				) : null}
				{entries.map((entry) => {
					const installAgent = showInstallAgent(entry);
					const installAcp = showInstallAcp(entry);
					const updateAgent = showUpdateAgent(entry);
					const uninstallAgent = showUninstallAgent(entry);
					// Install/ACP-only gaps gate “Use default”; Update/Uninstall can sit beside it.
					const needsInstall = installAgent || installAcp;
					const hasLifecycleAction =
						needsInstall || updateAgent || uninstallAgent;
					const notInstalled = !entry.binaryAvailable;
					const rowInstalling = lifecycleBusyIds.has(entry.templateId);
					const rowBusyAction = lifecycleBusyIds.get(entry.templateId);
					const rowLifecycle = lifecycleProgress[entry.templateId];
					// Mid-probe or host-cleared not-probed while a batch is running.
					const isProbing =
						probingKeys.has(catalogProbeKey(entry.templateId)) ||
						(entry.acpCommandAvailable &&
							entry.acpStatus === "not-probed" &&
							(loading || probingKeys.size > 0));
					return (
						<div
							key={entry.templateId}
							className="flex flex-col gap-2 border-b py-2.5 pr-1.5 pl-3.5 last:border-b-0"
						>
							<div className="flex items-center justify-between gap-3">
								<div className="flex min-w-0 flex-1 items-center gap-4">
									<div className="flex w-32 shrink-0 items-center gap-2">
										<AgentLogo template={entry.templateId} />
										<span
											className={cn(
												"min-w-0 truncate font-medium text-[13px]",
												// Dim label only — never the Install button (looks disabled).
												notInstalled &&
													!hasLifecycleAction &&
													"text-muted-foreground opacity-50",
												notInstalled &&
													hasLifecycleAction &&
													"text-muted-foreground",
											)}
										>
											{entry.name}
										</span>
									</div>
									<div className="flex min-w-0 flex-wrap items-center gap-1.5">
										{entry.isDefault ? (
											<StatusBadge tone="primary">
												{t("agent.badges.default")}
											</StatusBadge>
										) : null}
										{/* Layer 1: Agent host CLI */}
										{entry.binaryAvailable ? (
											<StatusBadge
												tone="ok"
												title={entry.resolvedPath ?? undefined}
											>
												{t("agent.badges.agentInstalled")}
											</StatusBadge>
										) : (
											<StatusBadge tone="muted">
												{t("agent.badges.agentMissing")}
											</StatusBadge>
										)}
										{/* Layer 2: ACP entrypoint / probe */}
										{!entry.acpCommandAvailable ? (
											<StatusBadge
												tone={entry.binaryAvailable ? "warn" : "muted"}
												title={
													entry.lastProbeError ?? entry.installHint ?? undefined
												}
											>
												{t("agent.badges.acpMissing")}
											</StatusBadge>
										) : isProbing ? (
											<ProbingBadge label={t("agent.probing")} />
										) : (
											<StatusBadge
												tone={catalogStatusTone(
													entry.acpStatus,
													entry.lastProbeError,
												)}
												title={
													entry.lastProbeError ??
													entry.acpAgentName ??
													undefined
												}
											>
												{acpStatusLabel(entry.acpStatus, entry.lastProbeError)}
											</StatusBadge>
										)}
									</div>
								</div>
								{/* Fixed action slot so icon-only rows align with “Use default” */}
								<div
									className={cn(
										"flex h-7 shrink-0 items-center justify-center gap-1",
										hasLifecycleAction ? "min-w-0" : "w-8",
									)}
								>
									{installAgent ? (
										<Button
											type="button"
											variant="outline"
											size="sm"
											className="h-7 gap-1 px-2 text-xs"
											aria-label={t("agent.installAgentAria", {
												name: entry.name,
											})}
											title={t("agent.installAgentTitle")}
											// Do not gate on global `busy` (catalog scan/ACP probe of
											// other agents) — that left Install looking dead for minutes.
											disabled={rowInstalling || !isTauri()}
											onClick={() => void onToolLifecycle(entry, "install")}
										>
											{rowBusyAction === "install" ? (
												<Loader2 className="size-3 animate-spin" />
											) : (
												<Terminal className="size-3" />
											)}
											{t("agent.installAgent")}
										</Button>
									) : null}
									{installAcp ? (
										<Button
											type="button"
											variant="outline"
											size="sm"
											className="h-7 gap-1 px-2 text-xs"
											aria-label={t("agent.installAdapterAria", {
												name: entry.name,
											})}
											title={t("agent.installAdapterTitle")}
											disabled={rowInstalling || !isTauri()}
											onClick={() => void onToolLifecycle(entry, "install")}
										>
											{rowBusyAction === "install" ? (
												<Loader2 className="size-3 animate-spin" />
											) : (
												<Terminal className="size-3" />
											)}
											{t("agent.installAdapter")}
										</Button>
									) : null}
									{updateAgent ? (
										<Button
											type="button"
											variant="ghost"
											size="sm"
											className="h-7 gap-1 px-2 text-xs"
											aria-label={t("agent.updateAgentAria", {
												name: entry.name,
											})}
											title={t("agent.updateAgentTitle")}
											disabled={rowInstalling || !isTauri()}
											onClick={() => void onToolLifecycle(entry, "update")}
										>
											{rowBusyAction === "update" ? (
												<Loader2 className="size-3 animate-spin" />
											) : (
												<ArrowUpCircle className="size-3" />
											)}
											{t("agent.updateAgent")}
										</Button>
									) : null}
									{uninstallAgent ? (
										<Button
											type="button"
											variant="ghost"
											size="icon-xs"
											className="size-7"
											aria-label={t("agent.uninstallAgentAria", {
												name: entry.name,
											})}
											title={t("agent.uninstallAgentTitle")}
											disabled={rowInstalling || !isTauri()}
											onClick={() =>
												openUninstallDialog({
													kind: "catalog",
													entry,
													info: null,
												})
											}
										>
											{rowBusyAction === "uninstall" ? (
												<Loader2
													className="size-3.5 animate-spin text-destructive"
													aria-hidden
												/>
											) : (
												<Trash2
													className="size-3.5 text-destructive"
													aria-hidden
												/>
											)}
										</Button>
									) : null}
								</div>
							</div>
							{rowLifecycle ? (
								<div className="grid grid-cols-[8rem_minmax(0,1fr)_2.5rem] items-center gap-3 pr-2">
									<span className="truncate text-[11px] text-muted-foreground">
										{rowLifecycle.detail}
									</span>
									<Progress
										value={rowLifecycle.progress ?? 0}
										className="h-1"
									/>
									<span className="text-right font-mono text-[10px] text-muted-foreground tabular-nums">
										{rowLifecycle.progress == null
											? ""
											: `${Math.round(rowLifecycle.progress)}%`}
									</span>
								</div>
							) : null}
						</div>
					);
				})}
				{customAgents.map((agent) => {
					const isDefault = catalog?.defaultId === agent.id;
					const notProbedYet = agent.available && agent.lastProbeOk == null;
					const isProbing =
						probingKeys.has(customProbeKey(agent.id)) ||
						(notProbedYet && (loading || probingKeys.size > 0));
					return (
						<div
							key={agent.id}
							className="flex items-center justify-between gap-3 border-b py-2.5 pr-1.5 pl-3.5 last:border-b-0"
						>
							<div className="flex min-w-0 flex-1 items-center gap-4">
								<div className="flex w-32 shrink-0 items-center gap-2">
									<AgentLogo template={agent.template} />
									<span className="min-w-0 truncate font-medium text-[13px]">
										{agent.name}
									</span>
								</div>
								<div className="flex min-w-0 flex-wrap items-center gap-1.5">
									{isDefault ? (
										<StatusBadge tone="primary">
											{t("agent.badges.default")}
										</StatusBadge>
									) : null}
									{isProbing ? (
										<ProbingBadge label={t("agent.probing")} />
									) : agent.lastProbeOk === true ? (
										<StatusBadge tone="ok">
											{t("agent:acpStatus.ready")}
										</StatusBadge>
									) : agent.lastProbeOk === false ? (
										<StatusBadge
											tone={
												isAgentAuthFailure(agent.lastProbeError)
													? "warn"
													: "err"
											}
											title={agent.lastProbeError ?? undefined}
										>
											{isAgentAuthFailure(agent.lastProbeError)
												? t("agent:acpStatus.notLoggedIn")
												: t("agent:acpStatus.failed")}
										</StatusBadge>
									) : notProbedYet ? (
										<ProbingBadge label={t("agent.probing")} />
									) : (
										<StatusBadge tone="muted">
											{t("agent:acpStatus.notInstalled")}
										</StatusBadge>
									)}
								</div>
							</div>
							<div className="flex h-7 w-20 shrink-0 items-center justify-center gap-1">
								<Button
									type="button"
									variant="ghost"
									size="icon-xs"
									className="size-7"
									aria-label={t("common:remove")}
									title={t("common:remove")}
									disabled={!isTauri()}
									onClick={() =>
										openUninstallDialog({
											kind: "custom",
											id: agent.id,
											name: agent.name,
											template: agent.template,
										})
									}
								>
									<Trash2 className="size-3.5 text-destructive" aria-hidden />
								</Button>
							</div>
						</div>
					);
				})}
				{/* Custom entry row — same row style as catalog agents; + expands the form */}
				<div className="flex items-center justify-between gap-3 border-b py-2.5 pr-1.5 pl-3.5 last:border-b-0">
					<div className="flex min-w-0 flex-1 items-center gap-4">
						<div className="flex w-32 shrink-0 items-center gap-2">
							<AgentLogo template="custom" />
							<span className="min-w-0 truncate font-medium text-[13px]">
								{t("agent.custom")}
							</span>
						</div>
					</div>
					<div className="flex h-7 w-20 shrink-0 items-center justify-center gap-1">
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							className="size-7"
							disabled={!isTauri()}
							aria-label={adding ? t("common:cancel") : t("agent.addCustom")}
							title={adding ? t("common:cancel") : t("agent.addCustom")}
							onClick={() => setAdding((v) => !v)}
						>
							{adding ? (
								<X className="size-3.5" aria-hidden />
							) : (
								<Plus className="size-3.5" aria-hidden />
							)}
						</Button>
					</div>
				</div>
				{adding ? (
					<div className="space-y-2.5 border-b px-3.5 py-3 last:border-b-0">
						<div className="space-y-1">
							<Label className="font-normal text-[13px]">
								{t("agent.form.name")}
							</Label>
							<Input
								value={formName}
								onChange={(e) => setFormName(e.target.value)}
								spellCheck={false}
							/>
						</div>
						<div className="space-y-1">
							<Label className="font-normal text-[13px]">
								{t("agent.form.command")}
							</Label>
							<Input
								value={formCommand}
								onChange={(e) => setFormCommand(e.target.value)}
								placeholder="opencode"
								spellCheck={false}
								autoComplete="off"
							/>
						</div>
						<div className="space-y-1">
							<Label className="font-normal text-[13px]">
								{t("agent.form.args")}
							</Label>
							<Input
								value={formArgs}
								onChange={(e) => setFormArgs(e.target.value)}
								placeholder="acp"
								spellCheck={false}
								autoComplete="off"
							/>
						</div>
						<div className="flex justify-end gap-1.5 pt-1">
							<Button
								type="button"
								variant="ghost"
								size="sm"
								onClick={() => setAdding(false)}
							>
								{t("common:cancel")}
							</Button>
							<Button
								type="button"
								size="sm"
								disabled={!formCommand.trim() || loading}
								onClick={() => void onAddCustom()}
							>
								{t("common:save")}
							</Button>
						</div>
					</div>
				) : null}
			</SettingsGroup>
			<p className="mt-2 mb-3 px-0.5 text-muted-foreground text-xs leading-relaxed">
				{t("agent.commonAgentsHint")}
			</p>

			<SettingsGroup>
				<div className="flex flex-col gap-1.5 px-3.5 py-2.5">
					<Label
						htmlFor="agent-personal-prompt"
						className="font-normal text-[13px]"
					>
						{t("agent.personalPrompt.label")}
					</Label>
					<Textarea
						id="agent-personal-prompt"
						value={settings.agentPersonalPrompt}
						onChange={(e) =>
							patch({
								agentPersonalPrompt: e.target.value.slice(0, 8000),
							})
						}
						onBlur={() => {
							const trimmed = settings.agentPersonalPrompt.trim();
							if (trimmed !== settings.agentPersonalPrompt) {
								patch({ agentPersonalPrompt: trimmed });
							}
						}}
						placeholder={t("agent.personalPrompt.placeholder")}
						rows={4}
						className="min-h-[88px] resize-y text-xs placeholder:text-muted-foreground/50"
						spellCheck={true}
					/>
				</div>
			</SettingsGroup>

			<p className="mb-1.5 mt-4 font-medium text-muted-foreground text-xs uppercase tracking-wide">
				{t("agent.pdfAsk.section")}
			</p>
			<SettingsGroup>
				<AgentModelPicker
					value={pdfAskValue}
					onChange={onPdfAskChange}
					registry={pdfAskRegistry}
					agentLabel={t("agent.pdfAsk.agentId.label")}
					modelLabel={t("agent.pdfAsk.modelId.label")}
					followDefaultLabel={t("agent.pdfAsk.agentId.followDefault")}
					followDefaultNamedLabel={(name) =>
						t("agent.pdfAsk.agentId.followDefaultNamed", { name })
					}
					followModelLabel={t("agent.pdfAsk.modelId.followAgent")}
					emptyState={
						<p className="px-3 py-2 text-muted-foreground text-xs">
							{t("agent.pdfAsk.agentId.empty")}
						</p>
					}
				/>
			</SettingsGroup>

			{/* Advanced / rare: mid-station User-Agent injection (#207). */}
			<p className="mb-1.5 mt-4 font-medium text-muted-foreground text-xs uppercase tracking-wide">
				{t("agent.userAgent.section")}
			</p>
			<SettingsGroup>
				<SettingsRow
					label={t("agent.userAgent.label")}
					htmlFor="agent-user-agent"
				>
					<div className="flex min-w-0 flex-col items-end gap-1.5">
						<div className="relative">
							<Input
								id="agent-user-agent"
								value={userAgentDraft}
								onChange={(e) => setUserAgentDraft(e.target.value)}
								onBlur={() => void commitUserAgent()}
								onKeyDown={(e) => {
									if (e.key === "Enter") {
										e.currentTarget.blur();
									}
								}}
								placeholder={t("agent.userAgent.placeholder")}
								spellCheck={false}
								autoComplete="off"
								disabled={!isTauri()}
								className="h-8 w-44 pr-7 text-xs sm:w-52"
							/>
							<DropdownMenu>
								<DropdownMenuTrigger asChild>
									<Button
										type="button"
										variant="ghost"
										size="icon-xs"
										className="absolute top-1/2 right-1 size-6 -translate-y-1/2"
										aria-label={t("agent.userAgent.presetsAria")}
										title={t("agent.userAgent.presetsAria")}
										disabled={!isTauri()}
									>
										<ChevronDown className="size-3.5" aria-hidden />
									</Button>
								</DropdownMenuTrigger>
								<DropdownMenuContent align="end">
									{USER_AGENT_PRESETS.map((preset) => (
										<DropdownMenuItem
											key={preset.id}
											onSelect={() =>
												void commitUserAgent({ userAgent: preset.value })
											}
										>
											{preset.value || t("agent.userAgent.off")}
										</DropdownMenuItem>
									))}
								</DropdownMenuContent>
							</DropdownMenu>
						</div>
					</div>
				</SettingsRow>
				<SettingsRow
					label={t("agent.userAgent.providerIdsLabel")}
					htmlFor="agent-user-agent-providers"
				>
					<Input
						id="agent-user-agent-providers"
						value={userAgentProviderDraft}
						onChange={(e) => setUserAgentProviderDraft(e.target.value)}
						onBlur={() => void commitUserAgent()}
						onKeyDown={(e) => {
							if (e.key === "Enter") {
								e.currentTarget.blur();
							}
						}}
						placeholder={t("agent.userAgent.providerIdsPlaceholder")}
						spellCheck={false}
						autoComplete="off"
						disabled={!isTauri() || !userAgentDraft.trim()}
						className="h-8 w-56 text-xs"
					/>
				</SettingsRow>
			</SettingsGroup>

			{/* Embedding endpoint (BYOK) for arxiv daily recommendation & semantic features. */}
			<div className="mb-1.5 mt-4 flex items-center justify-between gap-2">
				<p className="font-medium text-muted-foreground text-xs uppercase tracking-wide">
					{t("agent.embedding.section")}
				</p>
				<div className="flex items-center gap-1.5">
					<Tooltip>
						<TooltipTrigger asChild>
							<span
								role="status"
								aria-label={t(
									`agent.embedding.probeStatus.${embProbeStatus}` as "agent.embedding.probeStatus.idle",
								)}
								className={cn(
									"size-1.5 shrink-0 rounded-full",
									embProbeDotClass(embProbeStatus),
								)}
							/>
						</TooltipTrigger>
						<TooltipContent>
							{t(
								`agent.embedding.probeStatus.${embProbeStatus}` as "agent.embedding.probeStatus.idle",
							)}
						</TooltipContent>
					</Tooltip>
					<Button
						type="button"
						variant="outline"
						size="sm"
						disabled={
							embProbeBusy || !embDraft.baseUrl.trim() || !embDraft.model.trim()
						}
						onClick={() => void runEmbProbe()}
					>
						{embProbeBusy ? (
							<Loader2 className="size-3.5 animate-spin" aria-hidden />
						) : null}
						{embProbeBusy
							? t("agent.embedding.testing")
							: t("agent.embedding.test")}
					</Button>
				</div>
			</div>
			<SettingsGroup>
				<SettingsRow
					label={t("agent.embedding.baseUrl.label")}
					htmlFor="agent-embedding-base-url"
				>
					<Input
						id="agent-embedding-base-url"
						value={embDraft.baseUrl}
						onChange={(e) =>
							setEmbDraft((prev) => ({ ...prev, baseUrl: e.target.value }))
						}
						onBlur={() => {
							const trimmed = embDraft.baseUrl.trim().replace(/\/+$/, "");
							if (trimmed !== settings.embedding.baseUrl) {
								commitEmbedding({ baseUrl: trimmed });
							}
						}}
						onKeyDown={(e) => {
							if (e.key === "Enter") e.currentTarget.blur();
						}}
						placeholder="https://api.openai.com/v1"
						spellCheck={false}
						autoComplete="off"
						className="h-8 w-56 font-mono text-xs placeholder:text-muted-foreground/50"
					/>
				</SettingsRow>
				<SettingsRow
					label={t("agent.embedding.apiKey.label")}
					htmlFor="agent-embedding-api-key"
				>
					<Input
						id="agent-embedding-api-key"
						type="password"
						value={embDraft.apiKey}
						onChange={(e) =>
							setEmbDraft((prev) => ({ ...prev, apiKey: e.target.value }))
						}
						onFocus={(e) => {
							// Select the mask so the next keystroke replaces it entirely.
							if (isTranslateApiKeyMask(embDraft.apiKey)) {
								e.currentTarget.select();
							}
						}}
						onBlur={() => {
							const next = embDraft.apiKey.trim();
							// Unchanged mask → send as-is so the Host keeps the stored secret.
							if (next !== settings.embedding.apiKey) {
								commitEmbedding({ apiKey: next });
							}
						}}
						onKeyDown={(e) => {
							if (e.key === "Enter") e.currentTarget.blur();
						}}
						placeholder="sk-…"
						spellCheck={false}
						autoComplete="off"
						className="h-8 w-56 font-mono text-xs placeholder:text-muted-foreground/50"
					/>
				</SettingsRow>
				<SettingsRow
					label={t("agent.embedding.model.label")}
					htmlFor="agent-embedding-model"
				>
					<Input
						id="agent-embedding-model"
						value={embDraft.model}
						onChange={(e) =>
							setEmbDraft((prev) => ({ ...prev, model: e.target.value }))
						}
						onBlur={() => {
							const trimmed = embDraft.model.trim();
							if (trimmed !== settings.embedding.model) {
								commitEmbedding({ model: trimmed });
							}
						}}
						onKeyDown={(e) => {
							if (e.key === "Enter") e.currentTarget.blur();
						}}
						placeholder="text-embedding-3-small"
						spellCheck={false}
						autoComplete="off"
						className="h-8 w-56 font-mono text-xs placeholder:text-muted-foreground/50"
					/>
				</SettingsRow>
			</SettingsGroup>

			<AgentUninstallDialog
				open={uninstallTarget !== null}
				name={
					uninstallTarget?.kind === "custom"
						? uninstallTarget.name
						: (uninstallTarget?.entry.name ?? "")
				}
				template={
					uninstallTarget?.kind === "custom"
						? uninstallTarget.template
						: uninstallTarget?.entry.templateId
				}
				info={uninstallTarget?.kind === "catalog" ? uninstallTarget.info : null}
				busy={uninstallBusy}
				onConfirm={() => void onUninstallConfirm()}
				onCancel={() => setUninstallTarget(null)}
			/>
		</>
	);
}

function AgentChoiceLabel({ choice }: { choice: DefaultAgentChoice }) {
	return (
		<span className="flex min-w-0 items-center gap-2">
			<AgentLogo template={choice.template} />
			<span className="min-w-0 truncate">{choice.name}</span>
		</span>
	);
}

/**
 * Agent settings when the active vault is remote: discover + ACP probe run on the
 * SSH host (not this machine). App-level prefs (permission, language) still apply.
 */
export function RemoteAgentPane({
	settings,
	patch,
	hostContext,
}: {
	settings: AppSettings;
	patch: (p: Partial<AppSettings>) => void;
	hostContext: Extract<SettingsHostContext, { kind: "remote" }>;
}) {
	const { t } = useTranslation(["settings", "agent", "common"]);
	const [entries, setEntries] = useState<CatalogEntry[]>([]);
	const [loading, setLoading] = useState(false);
	const { probingKeys, setProbingKeys, clearProbingKey, clearAllProbingKeys } =
		useProbingKeys();
	const sessionId = hostContext.sessionId;

	const scanOnce = useCallback(async (): Promise<CatalogEntry[] | null> => {
		if (!isTauri()) {
			notifyError(t("agent.desktopOnly"));
			return null;
		}
		try {
			const scan = await remoteAgentScan(sessionId);
			setEntries(scan.entries);
			return scan.entries;
		} catch (e) {
			notifyError(errorText(e));
			return null;
		}
	}, [sessionId, t]);

	const patchEntryProbe = useCallback(
		(templateId: string, result: ProbeResult) => {
			setEntries((prev) =>
				prev.map((entry) => {
					if (entry.templateId !== templateId) return entry;
					return {
						...entry,
						acpStatus: result.available ? "ready" : "failed",
						acpAgentName: result.agentName ?? null,
						lastProbeError: result.error ?? null,
						lastProbedAt: new Date().toISOString(),
					};
				}),
			);
		},
		[],
	);

	const probeInstalled = useCallback(
		async (list: CatalogEntry[], force: boolean) => {
			if (!isTauri()) return;
			const candidates = list.filter((e) => catalogNeedsProbe(e, force));
			if (candidates.length === 0) {
				clearAllProbingKeys();
				return;
			}
			setProbingKeys(
				new Set(candidates.map((e) => catalogProbeKey(e.templateId))),
			);
			await Promise.allSettled(
				candidates.map(async (entry) => {
					const key = catalogProbeKey(entry.templateId);
					try {
						const result = await remoteAgentProbe(sessionId, entry.templateId);
						patchEntryProbe(entry.templateId, result);
					} catch (e) {
						const err = errorText(e);
						patchEntryProbe(entry.templateId, {
							agentId: entry.templateId,
							available: false,
							error: err,
						});
					} finally {
						clearProbingKey(key);
					}
				}),
			);
		},
		[
			sessionId,
			patchEntryProbe,
			clearProbingKey,
			clearAllProbingKeys,
			setProbingKeys,
		],
	);

	const rescanAndProbe = useCallback(
		async (force = false) => {
			if (!isTauri()) {
				notifyError(t("agent.desktopOnly"));
				return;
			}
			setLoading(true);
			try {
				const list = await scanOnce();
				if (list) await probeInstalled(list, force);
			} finally {
				setLoading(false);
				clearAllProbingKeys();
			}
		},
		[probeInstalled, scanOnce, t, clearAllProbingKeys],
	);

	// Soft probe when remote session (or rescan callback) changes.
	useEffect(() => {
		void rescanAndProbe(false);
	}, [rescanAndProbe]);

	const onInstallAdapter = async (entry: CatalogEntry) => {
		if (!isTauri()) return;
		try {
			await remoteAgentOpenInstallTerminal(sessionId, entry.templateId);
		} catch (e) {
			notifyError(errorText(e));
		}
	};

	const busy = loading;

	return (
		<>
			<PageTitle title={t("agent.title")} />
			<p className="mb-3 rounded-lg border border-sky-500/25 bg-sky-500/5 px-3 py-2 text-muted-foreground text-xs leading-relaxed">
				{t("agent.remote.banner", {
					host: hostContext.label,
					path: hostContext.remotePath || "—",
				})}
			</p>

			<SettingsGroup>
				<AgentCommonRows settings={settings} patch={patch} idSuffix="-r" />
			</SettingsGroup>

			{!isTauri() ? (
				<p className="mb-3 text-muted-foreground text-xs">
					{t("agent.desktopHint")}
				</p>
			) : null}

			<div className="mb-2 flex items-center justify-between gap-2">
				<p className="font-medium text-muted-foreground text-xs uppercase tracking-wide">
					{t("agent.remote.commonAgents")}
				</p>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					aria-label={t("agent.probe")}
					title={t("agent.remote.probeTitle")}
					disabled={busy || !isTauri()}
					onClick={() => void rescanAndProbe(true)}
				>
					{busy ? (
						<Loader2 className="size-3.5 animate-spin" aria-hidden />
					) : (
						<RefreshCw className="size-3.5" aria-hidden />
					)}
				</Button>
			</div>

			<SettingsGroup>
				{entries.length === 0 && busy ? (
					<div className="flex items-center gap-2 px-3.5 py-4 text-muted-foreground text-xs">
						<Loader2 className="size-3.5 animate-spin" aria-hidden />
						{t("agent.scanning")}
					</div>
				) : null}
				{entries.length === 0 && !busy ? (
					<p className="px-3.5 py-3 text-muted-foreground text-xs">
						{t("agent.remote.empty")}
					</p>
				) : null}
				{entries.map((entry) => {
					// Remote: only guided terminal ACP install (no silent host install).
					const installAcp = Boolean(entry.offerInstall);
					const notInstalled = !entry.binaryAvailable;
					const isProbing =
						probingKeys.has(catalogProbeKey(entry.templateId)) ||
						(entry.acpCommandAvailable &&
							entry.acpStatus === "not-probed" &&
							(loading || probingKeys.size > 0));
					return (
						<div
							key={entry.templateId}
							className="flex items-center justify-between gap-3 border-b py-2.5 pr-1.5 pl-3.5 last:border-b-0"
						>
							<div className="flex min-w-0 flex-1 items-center gap-4">
								<div className="flex w-32 shrink-0 items-center gap-2">
									<AgentLogo template={entry.templateId} />
									<span
										className={cn(
											"min-w-0 truncate font-medium text-[13px]",
											notInstalled && "text-muted-foreground",
											notInstalled && !installAcp && "opacity-50",
										)}
										title={
											entry.lastProbeError || entry.description || entry.name
										}
									>
										{entry.name}
									</span>
								</div>
								<div className="flex min-w-0 flex-wrap items-center gap-1.5">
									{entry.binaryAvailable ? (
										<StatusBadge
											tone="ok"
											title={entry.resolvedPath ?? undefined}
										>
											{t("agent.badges.agentInstalled")}
										</StatusBadge>
									) : (
										<StatusBadge tone="muted">
											{t("agent.badges.agentMissing")}
										</StatusBadge>
									)}
									{!entry.acpCommandAvailable ? (
										<StatusBadge
											tone={entry.binaryAvailable ? "warn" : "muted"}
											title={entry.lastProbeError ?? undefined}
										>
											{t("agent.badges.acpMissing")}
										</StatusBadge>
									) : isProbing ? (
										<ProbingBadge label={t("agent.probing")} />
									) : (
										<StatusBadge
											tone={catalogStatusTone(
												entry.acpStatus,
												entry.lastProbeError,
											)}
											title={
												entry.lastProbeError ?? entry.acpAgentName ?? undefined
											}
										>
											{acpStatusLabel(entry.acpStatus, entry.lastProbeError)}
										</StatusBadge>
									)}
								</div>
							</div>
							<div
								className={cn(
									"flex h-7 shrink-0 items-center justify-center gap-1",
									installAcp ? "min-w-0" : "w-8",
								)}
							>
								{installAcp ? (
									<Button
										type="button"
										variant="outline"
										size="sm"
										className="h-7 gap-1 px-2 text-xs"
										aria-label={t("agent.remote.installAdapterAria", {
											name: entry.name,
										})}
										title={
											entry.installCommand
												? t("agent.remote.installAdapterTitle", {
														command: entry.installCommand,
														host: hostContext.label,
													})
												: t("agent.installAdapter")
										}
										disabled={busy || !isTauri()}
										onClick={() => void onInstallAdapter(entry)}
									>
										<Terminal className="size-3" />
										{t("agent.installAdapter")}
									</Button>
								) : null}
							</div>
						</div>
					);
				})}
			</SettingsGroup>
			<p className="mt-2 mb-3 px-0.5 text-muted-foreground text-xs leading-relaxed">
				{t("agent.remote.hint")}
			</p>
		</>
	);
}
