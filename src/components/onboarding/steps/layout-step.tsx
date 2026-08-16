import {
	ArrowLeft,
	ExternalLink,
	KeyRound,
	LoaderCircle,
	Sparkles,
} from "lucide-react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChoiceCard } from "@/components/onboarding/choice-card";
import type { OnboardingStepId } from "@/components/onboarding/flow";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { isTauri } from "@/lib/core/tauri";
import { cn } from "@/lib/core/utils";
import {
	invokeLayoutRemoteProbe,
	tinyProbeJpegBase64,
} from "@/lib/pdf/layout/paddle";
import {
	isLayoutApiKeyMask,
	LAYOUT_PROVIDER_DOCS_URLS,
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

type ProbeStatus = "idle" | "probing" | "ok" | "fail";

type ProbeLabelKey =
	| "layout.probeOk"
	| "layout.probeFail"
	| "layout.probeProbing"
	| "layout.probeIdle";

function probeDotClass(status: ProbeStatus): string {
	switch (status) {
		case "ok":
			return "bg-emerald-500";
		case "fail":
			return "bg-destructive";
		case "probing":
			return "bg-amber-500 animate-pulse";
		default:
			return "bg-muted-foreground/35";
	}
}

function probeStatusLabelKey(status: ProbeStatus): ProbeLabelKey {
	switch (status) {
		case "ok":
			return "layout.probeOk";
		case "fail":
			return "layout.probeFail";
		case "probing":
			return "layout.probeProbing";
		default:
			return "layout.probeIdle";
	}
}

export function LayoutStep({
	settings,
	patch,
	onUseDefault,
	onNextChange,
}: {
	settings: AppSettings;
	patch: (p: Partial<AppSettings>) => void;
	/** User picked "use system default" — wizard should advance. */
	onUseDefault: () => void;
	/** Report whether a choice has been made (gates Next). */
	onNextChange: (id: OnboardingStepId, allowed: boolean) => void;
}) {
	const { t } = useTranslation(["onboarding", "settings"]);
	const layout = settings.layout;
	const stored = layout.providerConfigs.paddle;
	const [mode, setMode] = useState<"choose" | "configure">("choose");
	const [draft, setDraft] = useState<string | undefined>(undefined);
	const [probe, setProbe] = useState<ProbeStatus>("idle");

	// Next is allowed once the user committed to the own-API flow ("use system
	// default" advances immediately from the chooser instead).
	useEffect(() => {
		onNextChange("layout", mode === "configure");
	}, [mode, onNextChange]);

	const useSystemDefault = () => {
		patch({ layout: { ...layout, backend: "local" } });
		onUseDefault();
	};

	const displayKey = draft ?? stored?.apiKey ?? "";

	const confirm = async () => {
		const apiKey = (draft ?? stored?.apiKey ?? "").trim();
		if (!apiKey) return;
		const toSave = { apiKey };
		const masked = isLayoutApiKeyMask(apiKey)
			? apiKey
			: maskLayoutApiKey(apiKey);
		const nextLayout = {
			...layout,
			backend: "paddle" as const,
			providerConfigs: {
				...layout.providerConfigs,
				paddle: { ...toSave, apiKey: masked },
			},
		};
		setDraft(masked);
		try {
			await saveSettingsAsync({ ...settings, layout: nextLayout });
		} catch {
			// Still mask the UI below.
		}
		patch({ layout: nextLayout });
		setProbe("probing");
		const base64 = tinyProbeJpegBase64();
		if (!base64 || !isTauri()) {
			setProbe("fail");
			return;
		}
		try {
			await invokeLayoutRemoteProbe({
				provider: "paddle",
				imageBase64: base64,
				apiKey: toSave.apiKey,
			});
			setProbe("ok");
		} catch {
			setProbe("fail");
		}
	};

	if (mode === "choose") {
		return (
			<div className="grid grid-cols-2 gap-3">
				<ChoiceCard
					icon={<KeyRound className="size-5 text-muted-foreground" />}
					title={t("layout.configureOwn")}
					onClick={() => setMode("configure")}
				/>
				<ChoiceCard
					icon={<Sparkles className="size-5 text-muted-foreground" />}
					title={t("layout.useDefault")}
					onClick={useSystemDefault}
				/>
			</div>
		);
	}

	return (
		<div className="space-y-3">
			<div className="flex items-center gap-2">
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					className="-ml-1.5 shrink-0"
					aria-label={t("actions.back")}
					title={t("actions.back")}
					onClick={() => setMode("choose")}
				>
					<ArrowLeft className="size-4" />
				</Button>
				<span className="shrink-0 text-muted-foreground text-xs">
					{t("layout.apiKeyPlaceholder")}
				</span>
				<Button
					type="button"
					variant="ghost"
					size="icon-xs"
					className="shrink-0"
					aria-label={t("layout.openDocs")}
					title={t("layout.openDocs")}
					onClick={() => openExternalUrl(LAYOUT_PROVIDER_DOCS_URLS.paddle)}
				>
					<ExternalLink className="size-4" />
				</Button>
				<div className="ml-auto flex items-center gap-2">
					<span
						role="status"
						aria-label={t(probeStatusLabelKey(probe))}
						title={t(probeStatusLabelKey(probe))}
						className={cn(
							"inline-block size-1.5 shrink-0 rounded-full",
							probeDotClass(probe),
						)}
					/>
					<Button
						type="button"
						size="sm"
						disabled={probe === "probing" || !displayKey.trim()}
						onClick={() => void confirm()}
					>
						{probe === "probing" ? (
							<LoaderCircle data-icon="inline-start" className="animate-spin" />
						) : null}
						{t("layout.test")}
					</Button>
				</div>
			</div>
			<Input
				value={displayKey}
				placeholder={t("layout.apiKeyPlaceholder")}
				onChange={(e) => setDraft(e.target.value)}
				className="h-8"
			/>
		</div>
	);
}
