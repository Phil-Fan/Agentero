import { getVersion } from "@tauri-apps/api/app";
import { openUrl } from "@tauri-apps/plugin-opener";
import { Download, LoaderCircle, RefreshCw, Terminal } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { CompactCodeBlock } from "@/components/ai-elements/code-block";
import {
	PageTitle,
	SettingsGroup,
	SettingsRow,
} from "@/components/settings/settings-layout";
import { Button } from "@/components/ui/button";
import {
	type CliInstallStatus,
	fetchCliInstallStatus,
	installCliCommand,
	uninstallCliCommand,
} from "@/lib/cli/api";
import { notifyError, notifySuccess } from "@/lib/core/notify";
import { isMacOS, isTauri } from "@/lib/core/tauri";
import {
	checkForUpdate,
	getUpdateSnapshot,
	installAvailableUpdate,
	subscribeUpdate,
	type UpdateSnapshot,
} from "@/lib/update";

/** Same as README / homebrew-agentero Formula (headless CLI, not the desktop cask). */
const CLI_BREW_INSTALL_COMMAND =
	"brew tap poco-ai/agentero\nbrew install agentero";

export function AboutPane() {
	const { t } = useTranslation("settings");
	const [version, setVersion] = useState<string>();
	const [update, setUpdate] = useState<UpdateSnapshot>(getUpdateSnapshot);
	const [cli, setCli] = useState<CliInstallStatus | null>(null);
	const [cliBusy, setCliBusy] = useState(false);
	const [cliLoading, setCliLoading] = useState(false);
	const isMac = useMemo(() => isMacOS(), []);

	const refreshCli = useCallback(async () => {
		if (!isTauri()) return;
		setCliLoading(true);
		try {
			const status = await fetchCliInstallStatus();
			setCli(status);
		} catch {
			notifyError(t("about.cli.statusFailed"));
		} finally {
			setCliLoading(false);
		}
	}, [t]);

	useEffect(() => {
		void getVersion()
			.then(setVersion)
			.catch(() => undefined);
	}, []);
	useEffect(() => subscribeUpdate(setUpdate), []);
	useEffect(() => {
		void refreshCli();
	}, [refreshCli]);

	const checking = update.phase === "checking";
	const installing =
		update.phase === "downloading" || update.phase === "installing";
	const onCheck = () => {
		void checkForUpdate();
	};
	const onInstall = () => {
		void installAvailableUpdate().then((next) => {
			if (next.phase === "error") {
				notifyError(t("about.update.installFailed"));
			}
		});
	};
	const onInstallCli = () => {
		setCliBusy(true);
		void installCliCommand()
			.then(async (res) => {
				setCli(res.status);
				await refreshCli();
				notifySuccess(
					res.action === "download-install"
						? t("about.cli.downloadInstallSuccess")
						: t("about.cli.installSuccess"),
				);
			})
			.catch(() => notifyError(t("about.cli.installFailed")))
			.finally(() => setCliBusy(false));
	};
	const onUninstallCli = () => {
		setCliBusy(true);
		void uninstallCliCommand()
			.then(async (res) => {
				setCli(res.status);
				await refreshCli();
				notifySuccess(t("about.cli.uninstallSuccess"));
			})
			.catch(() => notifyError(t("about.cli.uninstallFailed")))
			.finally(() => setCliBusy(false));
	};
	const onOpenCliRelease = () => {
		const url = cli?.releasePageUrl;
		if (!url) return;
		void openUrl(url).catch(() =>
			notifyError(t("about.cli.openReleaseFailed")),
		);
	};

	const description = (() => {
		switch (update.phase) {
			case "unsupported":
				return t("about.update.unsupported");
			case "checking":
				return t("about.update.checking");
			case "up-to-date":
				return t("about.update.upToDate");
			case "available":
				return t("about.update.available", {
					version: update.availableVersion,
				});
			case "downloading":
				return update.totalBytes && update.downloadedBytes !== undefined
					? t("about.update.downloadingProgress", {
							progress: Math.min(
								100,
								Math.round((update.downloadedBytes / update.totalBytes) * 100),
							),
						})
					: t("about.update.downloading");
			case "installing":
				return t("about.update.installing");
			case "error":
				return t(
					update.errorOperation === "install"
						? "about.update.installFailed"
						: "about.update.checkFailed",
				);
			default:
				return t("about.update.idle");
		}
	})();

	const cliDescription = (() => {
		if (!cli) {
			return cliLoading ? "…" : t("about.cli.statusFailed");
		}
		if (cli.message?.trim()) {
			return cli.message;
		}
		if (cli.installed && !cli.shimCurrent) {
			return t("about.cli.versionMismatch", {
				cli: cli.cliVersion ?? "?",
				app: cli.appVersion,
			});
		}
		return t("about.cli.description");
	})();

	const needsCliUpdate = Boolean(
		cli?.installed && !cli.shimCurrent && cli.canInstall,
	);
	const canInstallCli = Boolean(cli?.canInstall) && !cliBusy;
	const showInstall = !cli?.installed || needsCliUpdate;
	const showBrewCliHint = isMac && showInstall && Boolean(cli);

	return (
		<>
			<PageTitle title={t("about.title")} />
			<SettingsGroup>
				<div className="space-y-1 px-3.5 py-4 text-center">
					<p className="font-semibold text-base tracking-tight">Agentero</p>
					{version && (
						<p className="text-muted-foreground text-sm">
							{t("about.version", { version })}
						</p>
					)}
					<p className="pt-2 text-muted-foreground text-xs leading-relaxed">
						{t("about.tagline")}
					</p>
				</div>
			</SettingsGroup>
			<SettingsGroup>
				<SettingsRow label={t("about.update.label")} description={description}>
					{update.phase === "available" ? (
						<Button size="sm" onClick={onInstall}>
							<Download data-icon="inline-start" />
							{t("about.update.downloadInstall")}
						</Button>
					) : update.phase === "unsupported" ? null : (
						<Button
							variant="outline"
							size="sm"
							disabled={checking || installing}
							onClick={onCheck}
						>
							{checking || installing ? (
								<LoaderCircle
									data-icon="inline-start"
									className="animate-spin"
								/>
							) : (
								<RefreshCw data-icon="inline-start" />
							)}
							{t("about.update.check")}
						</Button>
					)}
				</SettingsRow>
				{update.phase === "available" && update.notes?.trim() ? (
					<div className="border-t px-3.5 py-2.5 text-muted-foreground text-xs leading-relaxed whitespace-pre-wrap">
						{update.notes.trim()}
					</div>
				) : null}
			</SettingsGroup>
			{isTauri() ? (
				<SettingsGroup>
					<SettingsRow
						label={
							<span className="inline-flex items-center gap-1.5">
								<Terminal
									className="size-3.5 shrink-0 text-muted-foreground"
									aria-hidden
								/>
								{t("about.cli.label")}
							</span>
						}
						description={cliDescription}
					>
						<div className="flex flex-wrap items-center justify-end gap-2">
							{cli?.installed ? (
								<Button
									variant="outline"
									size="sm"
									disabled={cliBusy || cliLoading}
									onClick={onUninstallCli}
								>
									{cliBusy ? (
										<LoaderCircle
											data-icon="inline-start"
											className="animate-spin"
										/>
									) : null}
									{t("about.cli.uninstall")}
								</Button>
							) : null}
							{showInstall ? (
								<Button
									size="sm"
									disabled={!canInstallCli || cliLoading}
									onClick={onInstallCli}
								>
									{cliBusy ? (
										<LoaderCircle
											data-icon="inline-start"
											className="animate-spin"
										/>
									) : (
										<Download data-icon="inline-start" />
									)}
									{needsCliUpdate
										? t("about.cli.update")
										: t("about.cli.install")}
								</Button>
							) : null}
							{!cli?.canInstall && cli?.releasePageUrl ? (
								<Button
									variant="outline"
									size="sm"
									disabled={cliLoading}
									onClick={onOpenCliRelease}
								>
									{t("about.cli.openRelease")}
								</Button>
							) : null}
						</div>
					</SettingsRow>
					{showBrewCliHint ? (
						<div className="border-t px-3.5 py-3">
							<p className="mb-2 text-muted-foreground text-xs leading-relaxed">
								{t("about.cli.brewHint")}
							</p>
							<CompactCodeBlock
								code={CLI_BREW_INSTALL_COMMAND}
								language="shell"
								wrap
								className="[&_pre]:opacity-75"
								copyButtonProps={{
									"aria-label": t("about.cli.brewCopy"),
									onCopy: () => notifySuccess(t("about.cli.brewCopied")),
									onError: () => notifyError(t("about.cli.brewCopyFailed")),
								}}
							/>
						</div>
					) : null}
				</SettingsGroup>
			) : null}
		</>
	);
}
