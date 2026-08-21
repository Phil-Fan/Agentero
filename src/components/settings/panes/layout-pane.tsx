import { ExternalLink } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	PageTitle,
	SettingsGroup,
	SettingsRow,
} from "@/components/settings/settings-layout";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
	Select,
	SelectContent,
	SelectItem,
	SelectTrigger,
	SelectValue,
} from "@/components/ui/select";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { isTauri } from "@/lib/core/tauri";
import { cn } from "@/lib/core/utils";
import {
	persistLayoutProviderConfig,
	probeLayoutProvider,
} from "@/lib/pdf/layout/provider-config";
import {
	LAYOUT_PROVIDERS,
	layoutProviderCard,
	layoutProviderFor,
	mergeProviderCards,
	type ProviderCardDescriptor,
	parserProviderFor,
} from "@/lib/pdf/layout/providers";
import {
	isLayoutBackend,
	isParserBackend,
	LAYOUT_PROVIDER_DEFAULT_BASE_URLS,
	LAYOUT_PROVIDER_DOCS_URLS,
	type LayoutProviderConfig,
	PARSER_BACKENDS,
	PROVIDER_MODEL_PRESETS,
} from "@/lib/pdf/layout/settings";
import type { AppSettings } from "@/lib/settings";

function openExternalUrl(url: string): void {
	void import("@tauri-apps/plugin-opener")
		.then(({ openUrl }) => openUrl(url))
		.catch(() => {
			window.open(url, "_blank", "noopener,noreferrer");
		});
}

const EMPTY_PROVIDER_CONFIG: LayoutProviderConfig = {
	apiKey: "",
	baseUrl: "",
	model: "",
	prompt: "",
};

type ProbeStatus = "idle" | "probing" | "ok" | "fail";

function probeDotClass(status: ProbeStatus, configured: boolean): string {
	switch (status) {
		case "ok":
			return "bg-emerald-500";
		case "fail":
			return "bg-destructive";
		case "probing":
			return "bg-amber-500 animate-pulse";
		default:
			return configured ? "bg-muted-foreground/50" : "bg-muted-foreground/35";
	}
}

function probeStatusLabelKey(status: ProbeStatus): string {
	switch (status) {
		case "ok":
			return "layout.providerConfig.probeOk";
		case "fail":
			return "layout.providerConfig.probeFail";
		case "probing":
			return "layout.providerConfig.probeProbing";
		default:
			return "layout.providerConfig.probeIdle";
	}
}

export function LayoutPane({
	settings,
	patch,
}: {
	settings: AppSettings;
	patch: (p: Partial<AppSettings>) => void;
}) {
	const { t } = useTranslation("settings");
	const layout = settings.layout;
	const layoutDescriptor = layoutProviderFor(layout.backend);
	// One card per provider: when both engines share a provider the merged
	// card still shows the model / prompt inputs the parser role needs.
	const cards = mergeProviderCards([
		layoutDescriptor ? layoutProviderCard(layoutDescriptor) : null,
		parserProviderFor(layout.parserBackend),
	]);

	return (
		<div className="space-y-6">
			<PageTitle title={t("layout.title")} />

			<SettingsGroup>
				<SettingsRow label={t("layout.backend.label")} htmlFor="layout-backend">
					<Select
						value={layout.backend}
						onValueChange={(value) => {
							if (isLayoutBackend(value)) {
								patch({ layout: { ...layout, backend: value } });
							}
						}}
					>
						<SelectTrigger
							id="layout-backend"
							size="sm"
							className="min-w-[200px] max-w-[280px]"
						>
							<SelectValue />
						</SelectTrigger>
						<SelectContent className="max-h-72">
							{Object.values(LAYOUT_PROVIDERS).map((descriptor) => (
								<SelectItem key={descriptor.id} value={descriptor.id}>
									{t(
										`layout.backend.${descriptor.id}` as "layout.backend.local",
									)}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</SettingsRow>
				<SettingsRow
					label={t("layout.parserBackend.label")}
					htmlFor="layout-parser-backend"
				>
					<Select
						value={layout.parserBackend}
						onValueChange={(value) => {
							if (isParserBackend(value)) {
								patch({ layout: { ...layout, parserBackend: value } });
							}
						}}
					>
						<SelectTrigger
							id="layout-parser-backend"
							size="sm"
							className="min-w-[200px] max-w-[280px]"
						>
							<SelectValue />
						</SelectTrigger>
						<SelectContent className="max-h-72">
							{PARSER_BACKENDS.map((backend) => (
								<SelectItem key={backend} value={backend}>
									{t(
										`layout.parserBackend.${backend}` as "layout.parserBackend.local",
									)}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</SettingsRow>
			</SettingsGroup>

			{cards.map((card) => (
				<ProviderConfigCard
					key={card.id}
					provider={card}
					settings={settings}
					patch={patch}
				/>
			))}
		</div>
	);
}

function ProviderConfigCard({
	provider,
	settings,
	patch,
}: {
	provider: ProviderCardDescriptor;
	settings: AppSettings;
	patch: (p: Partial<AppSettings>) => void;
}) {
	const { t } = useTranslation("settings");
	const layout = settings.layout;
	const stored = layout.providerConfigs[provider.id] ?? EMPTY_PROVIDER_CONFIG;
	const [draft, setDraft] = useState<Partial<LayoutProviderConfig>>({});
	const [status, setStatus] = useState<ProbeStatus>("idle");
	const probeAbortRef = useRef<AbortController | null>(null);

	const displayApiKey =
		draft.apiKey !== undefined ? draft.apiKey : stored.apiKey;
	const displayBaseUrl =
		draft.baseUrl !== undefined ? draft.baseUrl : stored.baseUrl;
	const displayModel = draft.model !== undefined ? draft.model : stored.model;
	const displayPrompt =
		draft.prompt !== undefined ? draft.prompt : stored.prompt;
	const modelPresets = PROVIDER_MODEL_PRESETS[provider.id] ?? [];
	const configured = displayApiKey.trim().length > 0;

	const runProbe = useCallback(
		(apiKey: string) => {
			if (!isTauri()) return;
			if (!apiKey.trim()) {
				setStatus("idle");
				return;
			}
			probeAbortRef.current?.abort();
			const ac = new AbortController();
			probeAbortRef.current = ac;
			setStatus("probing");
			// Mask → Host resolves the stored token; plaintext draft → test pre-save.
			void probeLayoutProvider(provider.id, apiKey).then((ok) => {
				if (!ac.signal.aborted) setStatus(ok ? "ok" : "fail");
			});
		},
		[provider.id],
	);

	/** Confirm: persist drafts (key kept secret by Host), mask UI, then probe. */
	const confirmProvider = useCallback(async () => {
		const apiKey = (
			draft.apiKey !== undefined ? draft.apiKey : stored.apiKey
		).trim();
		const baseUrl = provider.supportsBaseUrl
			? (draft.baseUrl !== undefined ? draft.baseUrl : stored.baseUrl).trim()
			: "";
		const model = provider.supportsModel
			? (draft.model !== undefined ? draft.model : stored.model).trim()
			: stored.model;
		const prompt = provider.supportsPrompt
			? (draft.prompt !== undefined ? draft.prompt : stored.prompt).trim()
			: stored.prompt;
		if (!apiKey) {
			setStatus("idle");
			return;
		}
		const { displayLayout } = await persistLayoutProviderConfig({
			settings,
			provider: provider.id,
			config: { apiKey, baseUrl, model, prompt },
		});
		patch({ layout: displayLayout });
		setDraft({});
		runProbe(apiKey);
	}, [draft, patch, provider, runProbe, settings, stored]);

	useEffect(() => {
		return () => {
			probeAbortRef.current?.abort();
		};
	}, []);

	return (
		<div>
			<h3 className="mb-2 px-0.5 font-medium text-sm">
				{t("layout.providerConfig.section")}
			</h3>
			<div className="rounded-lg border bg-card px-3 py-2.5">
				<div className="mb-2 flex items-center justify-between gap-2">
					<div className="flex min-w-0 items-center gap-1.5">
						<Tooltip>
							<TooltipTrigger asChild>
								<span
									role="status"
									aria-label={t(
										probeStatusLabelKey(
											status,
										) as "layout.providerConfig.probeIdle",
									)}
									className={cn(
										"inline-block size-1.5 shrink-0 rounded-full",
										probeDotClass(status, configured),
									)}
								/>
							</TooltipTrigger>
							<TooltipContent>
								{t(
									probeStatusLabelKey(
										status,
									) as "layout.providerConfig.probeIdle",
								)}
							</TooltipContent>
						</Tooltip>
						<span className="truncate font-medium text-sm">
							{t(
								`layout.providerConfig.providerName.${provider.id}` as "layout.providerConfig.providerName.paddle",
							)}
						</span>
						<Button
							type="button"
							variant="link"
							size="xs"
							className="-ml-1.5 h-auto shrink-0 px-1.5 text-primary"
							onClick={() =>
								openExternalUrl(LAYOUT_PROVIDER_DOCS_URLS[provider.id])
							}
						>
							<ExternalLink data-icon="inline-start" className="size-3" />
							{t("layout.providerConfig.openDocsLabel")}
						</Button>
					</div>
					<Button
						type="button"
						variant="outline"
						size="xs"
						disabled={status === "probing"}
						onClick={() => void confirmProvider()}
					>
						{t("layout.providerConfig.confirm")}
					</Button>
				</div>

				<div className="space-y-2">
					{provider.requiresApiKey ? (
						<div className="flex items-center gap-2">
							<Label
								htmlFor={`layout-provider-${provider.id}-api-key`}
								className="w-20 shrink-0 font-normal text-muted-foreground text-xs"
							>
								{t("layout.providerConfig.apiKey.label")}
							</Label>
							<Input
								id={`layout-provider-${provider.id}-api-key`}
								type="password"
								value={displayApiKey}
								placeholder={t(
									`layout.providerConfig.apiKey.placeholder.${provider.id}` as "layout.providerConfig.apiKey.placeholder.paddle",
								)}
								className="h-8 min-w-0 flex-1 font-mono text-xs placeholder:text-muted-foreground/50"
								spellCheck={false}
								autoComplete="off"
								onChange={(e) =>
									setDraft((prev) => ({ ...prev, apiKey: e.target.value }))
								}
								onFocus={(e) => e.target.select()}
							/>
						</div>
					) : null}
					{provider.supportsBaseUrl ? (
						<div className="flex items-center gap-2">
							<Label
								htmlFor={`layout-provider-${provider.id}-base-url`}
								className="w-20 shrink-0 font-normal text-muted-foreground text-xs"
							>
								{t("layout.providerConfig.baseUrl.label")}
							</Label>
							<Input
								id={`layout-provider-${provider.id}-base-url`}
								type="text"
								value={displayBaseUrl}
								placeholder={LAYOUT_PROVIDER_DEFAULT_BASE_URLS[provider.id]}
								className="h-8 min-w-0 flex-1 font-mono text-xs placeholder:text-muted-foreground/50"
								spellCheck={false}
								autoComplete="off"
								onChange={(e) =>
									setDraft((prev) => ({ ...prev, baseUrl: e.target.value }))
								}
							/>
						</div>
					) : null}
					{provider.supportsModel ? (
						<div className="flex items-center gap-2">
							<Label
								htmlFor={`layout-provider-${provider.id}-model`}
								className="w-20 shrink-0 font-normal text-muted-foreground text-xs"
							>
								{t("layout.providerConfig.model.label")}
							</Label>
							<Input
								id={`layout-provider-${provider.id}-model`}
								type="text"
								value={displayModel}
								placeholder={modelPresets[0]}
								list={`layout-provider-${provider.id}-model-presets`}
								className="h-8 min-w-0 flex-1 font-mono text-xs placeholder:text-muted-foreground/50"
								spellCheck={false}
								autoComplete="off"
								onChange={(e) =>
									setDraft((prev) => ({ ...prev, model: e.target.value }))
								}
							/>
							<datalist id={`layout-provider-${provider.id}-model-presets`}>
								{modelPresets.map((preset) => (
									<option key={preset} value={preset} />
								))}
							</datalist>
						</div>
					) : null}
					{provider.supportsPrompt ? (
						<div className="flex items-center gap-2">
							<Label
								htmlFor={`layout-provider-${provider.id}-prompt`}
								className="w-20 shrink-0 font-normal text-muted-foreground text-xs"
							>
								{t("layout.providerConfig.prompt.label")}
							</Label>
							<Input
								id={`layout-provider-${provider.id}-prompt`}
								type="text"
								value={displayPrompt}
								placeholder={t("layout.providerConfig.prompt.placeholder")}
								className="h-8 min-w-0 flex-1 font-mono text-xs placeholder:text-muted-foreground/50"
								spellCheck={false}
								autoComplete="off"
								onChange={(e) =>
									setDraft((prev) => ({ ...prev, prompt: e.target.value }))
								}
							/>
						</div>
					) : null}
				</div>
			</div>
		</div>
	);
}
