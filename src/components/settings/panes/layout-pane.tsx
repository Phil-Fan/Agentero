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
	invokeLayoutRemoteProbe,
	tinyProbeJpegBase64,
} from "@/lib/pdf/layout/paddle";
import {
	isLayoutApiKeyMask,
	isLayoutBackend,
	LAYOUT_BACKENDS,
	LAYOUT_PROVIDER_DOCS_URLS,
	type LayoutBackend,
	type LayoutProviderConfig,
	maskLayoutApiKey,
} from "@/lib/pdf/layout/settings";
import type { AppSettings } from "@/lib/settings";
import { saveSettingsAsync } from "@/lib/settings";

function openExternalUrl(url: string): void {
	void import("@tauri-apps/plugin-opener")
		.then(({ openUrl }) => openUrl(url))
		.catch(() => {
			window.open(url, "_blank", "noopener,noreferrer");
		});
}

const EMPTY_PROVIDER_CONFIG: LayoutProviderConfig = {
	apiKey: "",
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
	const patchLayout = useCallback(
		(partial: Partial<typeof layout>) =>
			patch({ layout: { ...layout, ...partial } }),
		[patch, layout],
	);

	const showConfig = layout.backend === "paddle";
	const stored = layout.providerConfigs.paddle ?? EMPTY_PROVIDER_CONFIG;
	const [draft, setDraft] = useState<Partial<LayoutProviderConfig>>({});
	const [status, setStatus] = useState<ProbeStatus>("idle");
	const probeAbortRef = useRef<AbortController | null>(null);

	const displayApiKey =
		draft.apiKey !== undefined ? draft.apiKey : stored.apiKey;
	const configured = displayApiKey.trim().length > 0;

	const runProbe = useCallback(
		(override?: { apiKey?: string }) => {
			if (!isTauri()) return;
			const apiKey = (override?.apiKey ?? displayApiKey).trim();
			if (!apiKey) {
				setStatus("idle");
				return;
			}
			probeAbortRef.current?.abort();
			const ac = new AbortController();
			probeAbortRef.current = ac;
			setStatus("probing");
			const base64 = tinyProbeJpegBase64();
			if (!base64) {
				setStatus("fail");
				return;
			}
			// Mask → Host resolves the stored token; plaintext draft → test pre-save.
			void invokeLayoutRemoteProbe({
				imageBase64: base64,
				apiKey: apiKey || undefined,
			})
				.then(() => {
					if (!ac.signal.aborted) setStatus("ok");
				})
				.catch(() => {
					if (!ac.signal.aborted) setStatus("fail");
				});
		},
		[displayApiKey],
	);

	/** Confirm: persist drafts (key kept secret by Host), mask UI, then probe. */
	const confirmProvider = useCallback(async () => {
		const apiKey = (
			draft.apiKey !== undefined ? draft.apiKey : stored.apiKey
		).trim();
		const toSave: LayoutProviderConfig = { apiKey };
		if (!toSave.apiKey) {
			setStatus("idle");
			return;
		}
		const displayMask = isLayoutApiKeyMask(toSave.apiKey)
			? toSave.apiKey
			: maskLayoutApiKey(toSave.apiKey);
		const nextLayout = {
			...layout,
			providerConfigs: { ...layout.providerConfigs, paddle: toSave },
		};
		setDraft({});
		try {
			await saveSettingsAsync({ ...settings, layout: nextLayout });
		} catch {
			// Still mask the UI below.
		}
		patch({
			layout: {
				...nextLayout,
				providerConfigs: {
					...nextLayout.providerConfigs,
					paddle: { ...toSave, apiKey: displayMask },
				},
			},
		});
		runProbe({ apiKey: toSave.apiKey });
	}, [draft, layout, patch, runProbe, settings, stored]);

	useEffect(() => {
		return () => {
			probeAbortRef.current?.abort();
		};
	}, []);

	return (
		<div className="space-y-6">
			<PageTitle title={t("layout.title")} />

			<SettingsGroup>
				<SettingsRow label={t("layout.backend.label")} htmlFor="layout-backend">
					<Select
						value={layout.backend}
						onValueChange={(value) => {
							if (isLayoutBackend(value)) {
								patchLayout({ backend: value as LayoutBackend });
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
							{LAYOUT_BACKENDS.map((backend) => (
								<SelectItem key={backend} value={backend}>
									{t(`layout.backend.${backend}` as "layout.backend.local")}
								</SelectItem>
							))}
						</SelectContent>
					</Select>
				</SettingsRow>
			</SettingsGroup>

			{showConfig ? (
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
									{t("layout.providerConfig.paddle")}
								</span>
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

						<Button
							type="button"
							variant="link"
							size="xs"
							className="-ml-1.5 mb-2 h-auto px-1.5 text-primary"
							onClick={() => openExternalUrl(LAYOUT_PROVIDER_DOCS_URLS.paddle)}
						>
							<ExternalLink data-icon="inline-start" className="size-3" />
							{t("layout.providerConfig.openDocsLabel")}
						</Button>

						<div className="flex items-center gap-2">
							<Label
								htmlFor="layout-provider-paddle-api-key"
								className="w-20 shrink-0 font-normal text-muted-foreground text-xs"
							>
								{t("layout.providerConfig.apiKey.label")}
							</Label>
							<Input
								id="layout-provider-paddle-api-key"
								type="password"
								value={displayApiKey}
								placeholder={t("layout.providerConfig.apiKey.placeholder")}
								className="h-8 min-w-0 flex-1 font-mono text-xs placeholder:text-muted-foreground/50"
								spellCheck={false}
								autoComplete="off"
								onChange={(e) =>
									setDraft((prev) => ({ ...prev, apiKey: e.target.value }))
								}
								onFocus={(e) => e.target.select()}
							/>
						</div>
					</div>
				</div>
			) : null}
		</div>
	);
}
