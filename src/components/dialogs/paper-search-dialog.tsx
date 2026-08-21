import { Download, Loader2 } from "lucide-react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { ScrollArea } from "@/components/ui/scroll-area";
import { useOverlayRegistration } from "@/hooks/use-overlay-registration";
import { cn } from "@/lib/core/utils";
import type { PaperSearchCandidate } from "@/lib/paper/lookup";
import type { PaperSearchDraftGroup } from "@/lib/shell/ui-store";

/** Shows the head of the title-search queue; picking one imports it. */
export function PaperSearchDialog({
	groups,
	onCancel,
	onConfirm,
}: {
	groups: PaperSearchDraftGroup[] | null;
	onCancel: () => void;
	onConfirm: (
		candidate: PaperSearchCandidate,
		parentDir: string,
	) => Promise<void>;
}) {
	const { t } = useTranslation("sidebar");
	const group = groups?.[0] ?? null;
	const open = group !== null;
	const [pick, setPick] = useState<{ query: string; index: number } | null>(
		null,
	);
	const [busy, setBusy] = useState(false);
	const selected = pick && pick.query === group?.query ? pick.index : 0;

	useOverlayRegistration("paper-search", open, () => {
		if (!busy) onCancel();
	});

	const confirm = async () => {
		const candidate = group?.candidates[selected];
		if (!group || !candidate || busy) return;
		setBusy(true);
		try {
			await onConfirm(candidate, group.parentDir);
		} finally {
			setBusy(false);
		}
	};

	return (
		<Dialog
			open={open}
			onOpenChange={(next) => {
				if (!next && !busy) onCancel();
			}}
		>
			<DialogContent
				className="w-[min(36rem,calc(100vw-2rem))] max-w-none overflow-hidden"
				aria-describedby={undefined}
			>
				<DialogHeader>
					<DialogTitle className="truncate">
						{t("lookup.searchTitle", { query: group?.query ?? "" })}
					</DialogTitle>
				</DialogHeader>
				{/* Radix's ScrollArea viewport is display:table, so it grows to fit
				    content — nowrap children would push the cards past the dialog. */}
				<ScrollArea className="max-h-[min(60vh,28rem)] w-full">
					<div className="w-full min-w-0 space-y-2 pr-3">
						{group?.candidates.map((candidate, index) => {
							const checked = index === selected;
							const meta = [
								candidate.authors.slice(0, 3).join(", "),
								candidate.year?.toString(),
								candidate.venue,
							]
								.filter(Boolean)
								.join(" · ");
							return (
								<button
									key={candidate.identifier}
									type="button"
									onClick={() => setPick({ query: group.query, index })}
									disabled={busy}
									className={cn(
										"flex w-full min-w-0 flex-col gap-1 rounded-lg border p-3 text-left transition-colors",
										checked
											? "border-primary/50 bg-accent/50"
											: "bg-card hover:bg-accent/40",
										busy && "pointer-events-none opacity-60",
									)}
								>
									<span className="font-medium text-sm leading-snug">
										{candidate.title}
									</span>
									{meta ? (
										<span className="text-muted-foreground text-xs leading-snug">
											{meta}
										</span>
									) : null}
									<span className="flex flex-wrap items-center gap-2 text-muted-foreground text-xs">
										<span className="break-all rounded bg-muted px-1.5 py-0.5 font-mono text-[10px]">
											{candidate.arxivId
												? `arXiv:${candidate.arxivId}`
												: candidate.doi}
										</span>
										{candidate.citationCount !== undefined ? (
											<span>
												{t("lookup.searchCitations", {
													count: candidate.citationCount,
												})}
											</span>
										) : null}
									</span>
								</button>
							);
						})}
					</div>
				</ScrollArea>
				<DialogFooter>
					<Button variant="ghost" onClick={onCancel} disabled={busy}>
						{t("lookup.searchCancel")}
					</Button>
					<Button
						className="gap-1.5"
						onClick={() => void confirm()}
						disabled={busy || !group?.candidates.length}
					>
						{busy ? (
							<Loader2 className="size-4 animate-spin" />
						) : (
							<Download className="size-4" />
						)}
						{t("lookup.searchConfirm")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
