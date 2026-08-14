/**
 * 广场（Plaza）center panel: the source overview, or one embedded source.
 *
 * Everything is derived from the {@link PLAZA_SOURCES} registry, so a new source
 * needs no changes here.
 */

import { ChevronRight } from "lucide-react";
import { useTranslation } from "react-i18next";
import { PlazaFeedsView } from "@/components/plaza/plaza-feeds-view";
import { PlazaSkillsView } from "@/components/plaza/plaza-skills-view";
import { PlazaWebFrame } from "@/components/plaza/plaza-web-frame";
import { cn } from "@/lib/core/utils";
import {
	PLAZA_SOURCES,
	type PlazaSource,
	plazaSourceForPath,
	plazaSourceLabel,
} from "@/lib/plaza";

function SourceCard({
	source,
	onOpen,
}: {
	source: PlazaSource;
	onOpen: (source: PlazaSource) => void;
}) {
	const { t } = useTranslation("sidebar");
	const Icon = source.icon;
	const available = Boolean(source.url || source.panel);
	const label = plazaSourceLabel(source);
	return (
		<button
			type="button"
			disabled={!available}
			onClick={() => onOpen(source)}
			className={cn(
				"group flex w-full items-start gap-3 rounded-lg border bg-background p-3 text-left transition-colors",
				available
					? "hover:border-foreground/20 hover:bg-muted/50"
					: "cursor-default opacity-60",
				"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
			)}
		>
			<Icon className="mt-0.5 size-5 shrink-0" />
			<span className="min-w-0 flex-1">
				<span className="flex items-center gap-1 font-medium text-sm">
					<span className="truncate">{label}</span>
					{available ? (
						<ChevronRight
							className="size-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
							aria-hidden
						/>
					) : null}
				</span>
				<span className="mt-0.5 block text-muted-foreground text-xs leading-snug">
					{available ? t(source.description) : t("plaza.comingSoon")}
				</span>
			</span>
		</button>
	);
}

export function PlazaView({
	path,
	onOpenSource,
	className,
}: {
	path: string;
	onOpenSource: (source: PlazaSource) => void;
	className?: string;
}) {
	const { t } = useTranslation("sidebar");
	const source = plazaSourceForPath(path);
	const sourceLabel = source ? plazaSourceLabel(source) : "";

	if (source?.panel === "skills") {
		return <PlazaSkillsView className={className} />;
	}

	if (source?.panel === "feeds") {
		return <PlazaFeedsView className={className} />;
	}

	if (source?.url) {
		return (
			<PlazaWebFrame
				homeUrl={source.url}
				embedOrigin={source.embedOrigin?.() ?? null}
				title={sourceLabel}
				className={className}
			/>
		);
	}

	if (source) {
		return (
			<div
				className={cn(
					"flex h-full items-center justify-center p-6 text-center text-muted-foreground text-sm",
					className,
				)}
			>
				{t("plaza.comingSoonFor", { label: sourceLabel })}
			</div>
		);
	}

	return (
		<div
			className={cn("agentero-scroll h-full overflow-y-auto p-4", className)}
		>
			<h1 className="font-medium text-sm">{t("plaza.plaza")}</h1>
			<p className="mt-1 text-muted-foreground text-xs">
				{t("plaza.plazaDescription")}
			</p>
			<div className="mt-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
				{PLAZA_SOURCES.map((item) => (
					<SourceCard key={item.id} source={item} onOpen={onOpenSource} />
				))}
			</div>
		</div>
	);
}
