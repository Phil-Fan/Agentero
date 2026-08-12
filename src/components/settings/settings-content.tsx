import {
	Bot,
	Info,
	Keyboard,
	Languages,
	LayoutTemplate,
	MonitorSmartphone,
	Paintbrush,
	SlidersHorizontal,
	Stethoscope,
	X,
} from "lucide-react";
import {
	lazy,
	Suspense,
	useCallback,
	useEffect,
	useId,
	useMemo,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import type {
	SettingsHostContext,
	SettingsSection,
} from "@/components/settings/types";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/core/utils";
import type { AppSettings } from "@/lib/settings";
import {
	getRemoteSessionMeta,
	isRemoteVaultHandle,
	remoteSessionIdFromHandle,
} from "@/lib/vault/remote/remote-vault";

// Panes load per section instead of riding along with the settings shell:
// importing them statically pulled the whole surface (agent pane + model picker
// + translate) into the first paint. The *current* section is preloaded next to
// the shell (see `preloadSettingsPane`), so only the sections you actually visit
// pay for a chunk — a lazy default pane would just move the stall after render.
const PANE_LOADERS = {
	general: () => import("@/components/settings/panes/general-pane"),
	appearance: () => import("@/components/settings/panes/appearance-pane"),
	agent: () => import("@/components/settings/panes/agent-pane"),
	translate: () => import("@/components/settings/panes/translate-pane"),
	layout: () => import("@/components/settings/panes/layout-pane"),
	doctor: () => import("@/components/settings/panes/doctor-pane"),
	keyboard: () => import("@/components/settings/panes/keyboard-pane"),
	"remote-access": () =>
		import("@/components/settings/panes/remote-access-pane"),
	about: () => import("@/components/settings/panes/about-pane"),
} satisfies Record<SettingsSection, () => Promise<unknown>>;

/**
 * Warm the chunk for `section` before the shell renders. Module promises are
 * cached, so the matching `lazy()` below resolves without a second request.
 */
export function preloadSettingsPane(section: string): Promise<unknown> {
	const load = PANE_LOADERS[section as SettingsSection] ?? PANE_LOADERS.general;
	return load();
}

const GeneralPane = lazy(() =>
	PANE_LOADERS.general().then((m) => ({ default: m.GeneralPane })),
);
const AppearancePane = lazy(() =>
	PANE_LOADERS.appearance().then((m) => ({ default: m.AppearancePane })),
);
const AgentPane = lazy(() =>
	PANE_LOADERS.agent().then((m) => ({ default: m.AgentPane })),
);
const RemoteAgentPane = lazy(() =>
	PANE_LOADERS.agent().then((m) => ({ default: m.RemoteAgentPane })),
);
const TranslatePane = lazy(() =>
	PANE_LOADERS.translate().then((m) => ({ default: m.TranslatePane })),
);
const LayoutPane = lazy(() =>
	PANE_LOADERS.layout().then((m) => ({ default: m.LayoutPane })),
);
const DoctorPane = lazy(() =>
	PANE_LOADERS.doctor().then((m) => ({ default: m.DoctorPane })),
);
const KeyboardPane = lazy(() =>
	PANE_LOADERS.keyboard().then((m) => ({ default: m.KeyboardPane })),
);
const RemoteAccessPane = lazy(() =>
	PANE_LOADERS["remote-access"]().then((m) => ({
		default: m.RemoteAccessPane,
	})),
);
const AboutPane = lazy(() =>
	PANE_LOADERS.about().then((m) => ({ default: m.AboutPane })),
);

const NAV: {
	id: SettingsSection;
	icon: typeof Bot;
}[] = [
	{ id: "general", icon: SlidersHorizontal },
	{ id: "appearance", icon: Paintbrush },
	{ id: "agent", icon: Bot },
	{ id: "translate", icon: Languages },
	{ id: "layout", icon: LayoutTemplate },
	{ id: "doctor", icon: Stethoscope },
	{ id: "keyboard", icon: Keyboard },
	{ id: "remote-access", icon: MonitorSmartphone },
	{ id: "about", icon: Info },
];

/** Reserves height while a pane chunk loads so the window does not jump. */
function PaneFallback() {
	return <div className="h-64" aria-hidden />;
}

type SettingsContentProps = {
	section: SettingsSection;
	onSectionChange: (section: SettingsSection) => void;
	settings: AppSettings;
	onChange: (next: AppSettings) => void;
	/** Renders a close (X) button when provided (modal mode). */
	onClose?: () => void;
	/** aria-labelledby id supplied by a dialog wrapper. */
	titleId?: string;
	/** Active vault path — remote handles switch Agent settings to the SSH host. */
	vaultPath?: string | null;
};

/** Settings navigation + panes; used by the native settings window and the modal fallback. */
export function SettingsContent({
	section,
	onSectionChange,
	settings,
	onChange,
	onClose,
	titleId,
	vaultPath = null,
}: SettingsContentProps) {
	const { t } = useTranslation(["settings", "common"]);
	const fallbackTitleId = useId();
	const headingId = titleId ?? fallbackTitleId;

	// Keep visited panes mounted (hidden when inactive) so switching sections
	// doesn't unmount/remount them — avoids re-running their load effects
	// (agent list, connector status, cache stats, dynamic imports) and makes
	// section switches instant instead of re-fetching on every visit.
	const [visitedSections, setVisitedSections] = useState<SettingsSection[]>([
		section,
	]);
	useEffect(() => {
		setVisitedSections((prev) =>
			prev.includes(section) ? prev : [...prev, section],
		);
	}, [section]);
	const contentScrollRef = useRef<HTMLDivElement>(null);
	// biome-ignore lint/correctness/useExhaustiveDependencies: section intentionally triggers a scroll-to-top on switch
	useEffect(() => {
		contentScrollRef.current?.scrollTo({ top: 0 });
	}, [section]);

	const hostContext = useMemo((): SettingsHostContext => {
		if (vaultPath && isRemoteVaultHandle(vaultPath)) {
			const sessionId = remoteSessionIdFromHandle(vaultPath);
			const meta = getRemoteSessionMeta();
			if (sessionId && meta && meta.sessionId === sessionId) {
				const label =
					meta.host.trim() ||
					meta.displayName.split(":")[0]?.trim() ||
					t("host.remote");
				return {
					kind: "remote",
					label,
					sessionId,
					host: meta.host,
					remotePath: meta.remotePath,
				};
			}
			if (sessionId) {
				return {
					kind: "remote",
					label: t("host.remote"),
					sessionId,
					host: "",
					remotePath: "",
				};
			}
		}
		return { kind: "local" };
	}, [vaultPath, t]);

	const patch = useCallback(
		(partial: Partial<AppSettings>) => onChange({ ...settings, ...partial }),
		[onChange, settings],
	);

	return (
		<>
			{/* Sidebar — macOS Settings style */}
			<nav className="flex w-[180px] shrink-0 flex-col border-r bg-muted/40">
				{/* Modal fallback only: native window already shows the title in its title bar. */}
				{onClose ? (
					<div className="flex items-center justify-between gap-1 px-3 pt-3 pb-2">
						<span
							id={headingId}
							className="font-semibold text-[13px] leading-none tracking-tight"
						>
							{t("title")}
						</span>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							className="shrink-0"
							aria-label={t("common:close")}
							onClick={onClose}
						>
							<X className="size-3.5" />
						</Button>
					</div>
				) : null}
				<ul
					className={cn(
						"agentero-scroll flex min-h-0 flex-1 flex-col gap-0.5 px-2 pb-2",
						!onClose && "pt-3",
					)}
				>
					{NAV.map((item) => {
						const Icon = item.icon;
						const active = section === item.id;
						return (
							<li key={item.id}>
								<button
									type="button"
									className={cn(
										"flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-[13px] outline-none transition-colors",
										"hover:bg-black/5 dark:hover:bg-white/10",
										active &&
											"bg-primary text-primary-foreground hover:bg-primary dark:hover:bg-primary",
									)}
									aria-current={active ? "page" : undefined}
									onClick={() => onSectionChange(item.id)}
								>
									<Icon className="size-3.5 shrink-0 opacity-90" />
									<span className="truncate">{t(`nav.${item.id}`)}</span>
								</button>
							</li>
						);
					})}
				</ul>
			</nav>

			{/* Content */}
			<div ref={contentScrollRef} className="agentero-scroll min-w-0 flex-1">
				<div className="px-6 py-5">
					{/* One boundary per pane: a section still loading must not blank out
					    the panes already visited and kept mounted next to it. */}
					{visitedSections.includes("general") && (
						<div hidden={section !== "general"}>
							<Suspense fallback={<PaneFallback />}>
								<GeneralPane
									settings={settings}
									patch={patch}
									hostContext={hostContext}
								/>
							</Suspense>
						</div>
					)}
					{visitedSections.includes("appearance") && (
						<div hidden={section !== "appearance"}>
							<Suspense fallback={<PaneFallback />}>
								<AppearancePane
									theme={settings.theme}
									uiTheme={settings.uiTheme}
									locale={settings.locale}
									uiScale={settings.uiScale}
									editorFontSize={settings.editorFontSize}
									interfaceFontFamily={settings.interfaceFontFamily}
									textFontFamily={settings.textFontFamily}
									monoFontFamily={settings.monoFontFamily}
									editorLineHeight={settings.editorLineHeight}
									showEditorToolbar={settings.showEditorToolbar}
									patch={patch}
								/>
							</Suspense>
						</div>
					)}
					{visitedSections.includes("agent") && (
						<div hidden={section !== "agent"}>
							<Suspense fallback={<PaneFallback />}>
								{hostContext.kind === "remote" ? (
									<RemoteAgentPane
										settings={settings}
										patch={patch}
										hostContext={hostContext}
									/>
								) : (
									<AgentPane settings={settings} patch={patch} />
								)}
							</Suspense>
						</div>
					)}
					{visitedSections.includes("translate") && (
						<div hidden={section !== "translate"}>
							<Suspense fallback={<PaneFallback />}>
								<TranslatePane
									settings={settings}
									patch={patch}
									onOpenAgentSettings={() => onSectionChange("agent")}
								/>
							</Suspense>
						</div>
					)}
					{visitedSections.includes("layout") && (
						<div hidden={section !== "layout"}>
							<Suspense fallback={<PaneFallback />}>
								<LayoutPane settings={settings} patch={patch} />
							</Suspense>
						</div>
					)}
					{visitedSections.includes("doctor") && (
						<div hidden={section !== "doctor"}>
							<Suspense fallback={<PaneFallback />}>
								<DoctorPane vaultPath={vaultPath} hostContext={hostContext} />
							</Suspense>
						</div>
					)}
					{visitedSections.includes("keyboard") && (
						<div hidden={section !== "keyboard"}>
							<Suspense fallback={<PaneFallback />}>
								<KeyboardPane />
							</Suspense>
						</div>
					)}
					{visitedSections.includes("remote-access") && (
						<div hidden={section !== "remote-access"}>
							<Suspense fallback={<PaneFallback />}>
								<RemoteAccessPane vaultPath={vaultPath} />
							</Suspense>
						</div>
					)}
					{visitedSections.includes("about") && (
						<div hidden={section !== "about"}>
							<Suspense fallback={<PaneFallback />}>
								<AboutPane />
							</Suspense>
						</div>
					)}
				</div>
			</div>
		</>
	);
}
