import { Download, LoaderCircle, Wifi } from "lucide-react";
import {
	forwardRef,
	useCallback,
	useEffect,
	useImperativeHandle,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { AgentLogo } from "@/components/agent/agent-logo";
import {
	type CatalogEntry,
	type CatalogScanResponse,
	ensureCatalogAgent,
	runToolLifecycle,
	scanCatalog,
} from "@/lib/agent/api";
import { cn } from "@/lib/core/utils";

/** Imperative handle so the wizard header can trigger a rescan. */
export type AgentStepHandle = {
	rescan: () => void;
};

function statusDotClass(status: CatalogEntry["acpStatus"]): string {
	switch (status) {
		case "ready":
			return "bg-emerald-500";
		case "failed":
			return "bg-destructive";
		case "not-probed":
			return "bg-muted-foreground/40";
		default:
			return "bg-muted-foreground/30";
	}
}

type StatusLabelKey =
	| "agent.ready"
	| "agent.failed"
	| "agent.notProbed"
	| "agent.missing";

function statusLabelKey(status: CatalogEntry["acpStatus"]): StatusLabelKey {
	switch (status) {
		case "ready":
			return "agent.ready";
		case "failed":
			return "agent.failed";
		case "not-probed":
			return "agent.notProbed";
		default:
			return "agent.missing";
	}
}

/** Probe-ready (installed / registered) agents go first; uninstalled sink below. */
function isAvailable(entry: CatalogEntry): boolean {
	return (
		entry.acpStatus === "ready" ||
		Boolean(entry.registeredId) ||
		entry.acpCommandAvailable
	);
}

function sortEntries(entries: CatalogEntry[]): CatalogEntry[] {
	return [...entries].sort((a, b) => {
		const diff = Number(isAvailable(b)) - Number(isAvailable(a));
		if (diff !== 0) return diff;
		return a.name.localeCompare(b.name);
	});
}

export const AgentStep = forwardRef<AgentStepHandle>(
	function AgentStep(_props, ref) {
		const { t } = useTranslation("onboarding");
		const [state, setState] = useState<CatalogScanResponse | null>(null);
		const [scanning, setScanning] = useState(false);
		const [error, setError] = useState<string | null>(null);
		const [busyId, setBusyId] = useState<string | null>(null);
		const mountedRef = useRef(true);

		useEffect(() => {
			mountedRef.current = true;
			return () => {
				mountedRef.current = false;
			};
		}, []);

		const runScan = useCallback(async () => {
			setScanning(true);
			setError(null);
			try {
				const res = await scanCatalog();
				if (mountedRef.current) setState(res);
			} catch {
				if (mountedRef.current) setError(t("agent.scanFailed"));
			} finally {
				if (mountedRef.current) setScanning(false);
			}
		}, [t]);

		useEffect(() => {
			void runScan();
		}, [runScan]);

		useImperativeHandle(
			ref,
			() => ({
				rescan: () => {
					void runScan();
				},
			}),
			[runScan],
		);

		const onSetDefault = async (entry: CatalogEntry) => {
			setBusyId(entry.templateId);
			try {
				await ensureCatalogAgent(entry.templateId, true);
				await runScan();
			} finally {
				if (mountedRef.current) setBusyId(null);
			}
		};

		const onInstall = async (entry: CatalogEntry) => {
			setBusyId(`install:${entry.templateId}`);
			setError(null);
			try {
				await runToolLifecycle(entry.templateId, "install");
				await runScan();
			} catch {
				if (mountedRef.current) setError(t("agent.installFailed"));
			} finally {
				if (mountedRef.current) setBusyId(null);
			}
		};

		const entries = sortEntries(state?.entries ?? []);

		return (
			<div className="space-y-4">
				{error ? <p className="text-destructive text-xs">{error}</p> : null}

				{entries.length === 0 ? (
					<div className="flex items-center gap-2 rounded-lg border border-dashed p-4 text-muted-foreground text-xs">
						<Wifi className="size-4 shrink-0" />
						<span>{scanning ? t("agent.scanning") : t("agent.none")}</span>
					</div>
				) : (
					<ul className="grid max-h-[20rem] grid-cols-4 gap-3 overflow-y-auto pr-0.5">
						{entries.map((entry) => {
							const available = isAvailable(entry);
							const isDefault =
								entry.isDefault || entry.registeredId === state?.defaultId;
							const busy =
								busyId === entry.templateId ||
								busyId === `install:${entry.templateId}`;
							return (
								<li key={entry.templateId}>
									<button
										type="button"
										aria-pressed={isDefault}
										aria-label={`${entry.name} — ${t(statusLabelKey(entry.acpStatus))}`}
										disabled={busy || !available}
										onClick={() => {
											if (available) void onSetDefault(entry);
										}}
										className={cn(
											"group relative flex w-full flex-col items-center gap-2 rounded-xl border bg-background p-3 text-center outline-none transition-colors hover:border-primary/60 focus-visible:border-ring focus-visible:ring-2 focus-visible:ring-ring/50",
											isDefault && "border-primary bg-primary/5",
											!available && "opacity-60",
										)}
									>
										{/* Top-right: "default" tag, status dot, or install icon. */}
										{isDefault ? (
											<span className="absolute top-2 right-2 rounded bg-primary px-1 py-0.5 text-[9px] leading-none font-medium text-primary-foreground">
												{t("agent.default")}
											</span>
										) : available ? (
											<span
												role="status"
												aria-label={t(statusLabelKey(entry.acpStatus))}
												className={cn(
													"absolute top-2 right-2 size-1.5 rounded-full",
													statusDotClass(entry.acpStatus),
												)}
											/>
										) : entry.canInstall ? (
											<button
												type="button"
												disabled={busy}
												aria-label={t("agent.install")}
												title={t("agent.install")}
												onClick={(e) => {
													e.stopPropagation();
													void onInstall(entry);
												}}
												className="absolute top-1.5 right-1.5 rounded p-0.5 text-muted-foreground transition-colors opacity-80 hover:bg-background hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring/50 group-hover:opacity-100"
											>
												{busy ? (
													<LoaderCircle className="size-3.5 animate-spin" />
												) : (
													<Download className="size-3.5" />
												)}
											</button>
										) : (
											<span
												role="status"
												aria-label={t(statusLabelKey(entry.acpStatus))}
												className="absolute top-2 right-2 size-1.5 rounded-full bg-muted-foreground/30"
											/>
										)}

										<div className="flex size-14 items-center justify-center">
											{busy ? (
												<LoaderCircle className="size-6 animate-spin" />
											) : (
												<AgentLogo
													template={entry.templateId}
													className="size-12 rounded-none border-0 bg-transparent shadow-none"
													iconClassName="size-7"
												/>
											)}
										</div>
										<p className="w-full truncate text-[13px] font-medium">
											{entry.name}
										</p>
									</button>
								</li>
							);
						})}
					</ul>
				)}
			</div>
		);
	},
);
