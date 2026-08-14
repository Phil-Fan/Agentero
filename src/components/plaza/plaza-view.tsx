/**
 * 广场（Plaza）center panel: the source overview, or one embedded source.
 *
 * Everything is derived from the {@link PLAZA_SOURCES} registry, so a new source
 * needs no changes here.
 */

import { ChevronRight } from "lucide-react";
import { PlazaWebFrame } from "@/components/plaza/plaza-web-frame";
import { cn } from "@/lib/core/utils";
import {
	PLAZA_SOURCES,
	type PlazaSource,
	plazaSourceForPath,
} from "@/lib/plaza";

function SourceCard({
	source,
	onOpen,
}: {
	source: PlazaSource;
	onOpen: (source: PlazaSource) => void;
}) {
	const Icon = source.icon;
	const available = Boolean(source.url);
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
					<span className="truncate">{source.label}</span>
					{available ? (
						<ChevronRight
							className="size-3.5 shrink-0 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100"
							aria-hidden
						/>
					) : null}
				</span>
				<span className="mt-0.5 block text-muted-foreground text-xs leading-snug">
					{available ? source.description : "即将推出"}
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
	const source = plazaSourceForPath(path);

	if (source?.url) {
		return (
			<PlazaWebFrame
				homeUrl={source.url}
				embedOrigin={source.embedOrigin?.() ?? null}
				title={source.label}
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
				{source.label} 即将推出。
			</div>
		);
	}

	return (
		<div
			className={cn("agentero-scroll h-full overflow-y-auto p-4", className)}
		>
			<h1 className="font-medium text-sm">广场</h1>
			<p className="mt-1 text-muted-foreground text-xs">
				从外部来源发现论文；入库后会出现在论文库中。
			</p>
			<div className="mt-3 grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
				{PLAZA_SOURCES.map((item) => (
					<SourceCard key={item.id} source={item} onOpen={onOpenSource} />
				))}
			</div>
		</div>
	);
}
