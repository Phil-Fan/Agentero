import { BookMarked } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { ReferencesPanel } from "@/components/viewer/panels/references-panel";

type PdfReferencesPanelProps = {
	vaultPath: string | null;
	paperPath: string | null;
	showReferences: boolean;
	onToggleReferences: () => void;
};

/** References toggle (top-left, next to outline) plus the left-side citation panel. */
export function PdfReferencesPanel({
	vaultPath,
	paperPath,
	showReferences,
	onToggleReferences,
}: PdfReferencesPanelProps) {
	const { t } = useTranslation("viewer");

	if (!paperPath) return null;

	return (
		<>
			<div className="pointer-events-none absolute top-2 left-12 z-30">
				<TooltipProvider delayDuration={200}>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								size="icon-xs"
								variant="ghost"
								className="pointer-events-auto rounded-lg border border-border/80 bg-background/95 shadow-sm backdrop-blur-sm"
								aria-label={t("references.title")}
								aria-pressed={showReferences}
								onClick={onToggleReferences}
							>
								<BookMarked className="size-3.5" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							{t("references.title")}
						</TooltipContent>
					</Tooltip>
				</TooltipProvider>
			</div>
			{showReferences ? (
				<aside className="agentero-scroll absolute inset-y-0 left-0 z-20 w-80 border-r bg-background/95 pt-11 pb-2 backdrop-blur-sm">
					<ReferencesPanel
						vaultPath={vaultPath}
						paperPath={paperPath}
						className="h-full"
						compact
					/>
				</aside>
			) : null}
		</>
	);
}
