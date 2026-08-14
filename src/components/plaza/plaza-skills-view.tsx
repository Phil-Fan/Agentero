/**
 * Native 广场 panel: curated research Skill repos as GitHub-style cards.
 * Card click runs the 魔棒 Skill import; the corner link opens GitHub.
 */

import { ExternalLink, Loader2, Star } from "lucide-react";
import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { openExternalUrl } from "@/lib/core/open-external";
import { cn } from "@/lib/core/utils";
import { importPlazaSkillRepo } from "@/lib/plaza/import";
import {
	SKILL_THEMES,
	type SkillRepo,
	type SkillThemeId,
} from "@/lib/plaza/skill-catalog";

function formatStars(n: number): string {
	if (n >= 1000) {
		const k = n / 1000;
		return `${k >= 10 ? k.toFixed(0) : k.toFixed(1).replace(/\.0$/, "")}k`;
	}
	return String(n);
}

function RepoCard({ repo }: { repo: SkillRepo }) {
	const { t } = useTranslation("sidebar");
	const [busy, setBusy] = useState(false);
	const fullName = `${repo.owner}/${repo.repo}`;

	const importRepo = useCallback(async () => {
		if (busy) return;
		setBusy(true);
		try {
			await importPlazaSkillRepo(repo.url);
		} finally {
			setBusy(false);
		}
	}, [busy, repo.url]);

	return (
		<div
			className={cn(
				"group relative rounded-lg border bg-background p-3",
				"transition-colors hover:border-foreground/20 hover:bg-muted/50",
			)}
		>
			<button
				type="button"
				disabled={busy}
				aria-label={t("plaza.skills.import", { name: fullName })}
				onClick={() => void importRepo()}
				className={cn(
					"flex w-full items-start gap-3 pr-7 text-left",
					"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
					"disabled:pointer-events-none disabled:opacity-70",
				)}
			>
				<img
					src={`https://avatars.githubusercontent.com/${repo.owner}?s=80`}
					alt=""
					width={40}
					height={40}
					className="size-10 shrink-0 rounded-md shadow-[inset_0_0_0_1px_rgba(0,0,0,0.08)]"
				/>
				<div className="min-w-0 flex-1">
					<div className="flex items-center gap-2">
						<span className="min-w-0 truncate font-medium text-sm">
							{fullName}
						</span>
						{busy ? (
							<Loader2
								className="size-3.5 shrink-0 animate-spin text-muted-foreground"
								aria-hidden
							/>
						) : null}
						<span className="ml-auto inline-flex shrink-0 items-center gap-0.5 text-muted-foreground text-xs">
							<Star className="size-3 fill-current" aria-hidden />
							{formatStars(repo.stars)}
						</span>
					</div>
					<p className="mt-0.5 line-clamp-2 text-muted-foreground text-xs leading-snug">
						{repo.description}
					</p>
				</div>
			</button>
			<Tooltip>
				<TooltipTrigger asChild>
					<button
						type="button"
						className={cn(
							"absolute top-2.5 right-2.5 inline-flex size-6 items-center justify-center rounded-sm",
							"text-muted-foreground opacity-0 transition-opacity",
							"hover:bg-muted hover:text-foreground group-hover:opacity-100",
							"focus-visible:opacity-100 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
						)}
						aria-label={t("plaza.skills.openGithub")}
						onClick={() => openExternalUrl(repo.url)}
					>
						<ExternalLink className="size-3.5" />
					</button>
				</TooltipTrigger>
				<TooltipContent side="top">
					{t("plaza.skills.openGithub")}
				</TooltipContent>
			</Tooltip>
		</div>
	);
}

export function PlazaSkillsView({ className }: { className?: string }) {
	const { t } = useTranslation("sidebar");
	return (
		<div
			className={cn("agentero-scroll h-full overflow-y-auto p-4", className)}
		>
			<h1 className="font-medium text-sm">{t("plaza.skills.title")}</h1>
			<p className="mt-1 text-muted-foreground text-xs">
				{t("plaza.skills.blurb")}
			</p>
			<div className="mt-4 space-y-6">
				{SKILL_THEMES.map((theme) => (
					<section key={theme.id}>
						<h2 className="mb-2 font-medium text-muted-foreground text-xs">
							{t(`plaza.skills.themes.${theme.id as SkillThemeId}`)}
						</h2>
						<div className="grid grid-cols-3 gap-2">
							{theme.repos.map((repo) => (
								<RepoCard key={repo.url} repo={repo} />
							))}
						</div>
					</section>
				))}
			</div>
		</div>
	);
}
