import { Boxes } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { FiguresPanel } from "@/components/viewer/panels/figures-panel";
import type { PdfLayoutRegion } from "@/lib/pdf/layout";

type PdfFiguresPanelProps = {
	documentId: string;
	showFigures: boolean;
	analyzing?: boolean;
	onToggleFigures: () => void;
	onAnalyze: () => void;
	onJump: (region: PdfLayoutRegion) => void;
	onRenderThumb: (region: PdfLayoutRegion) => Promise<{
		mimeType: string;
		data: string;
	} | null>;
};

/** Figures toggle (top-left, next to references) plus the left-side layout panel. */
export function PdfFiguresPanel({
	documentId,
	showFigures,
	analyzing,
	onToggleFigures,
	onAnalyze,
	onJump,
	onRenderThumb,
}: PdfFiguresPanelProps) {
	const { t } = useTranslation("viewer");

	return (
		<>
			<div className="pointer-events-none absolute top-2 left-[5.25rem] z-30">
				<TooltipProvider delayDuration={200}>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								size="icon-xs"
								variant="ghost"
								className="pointer-events-auto rounded-lg border border-border/80 bg-background/95 shadow-sm backdrop-blur-sm"
								aria-label={t("figures.title")}
								aria-pressed={showFigures}
								disabled={analyzing}
								onClick={onToggleFigures}
							>
								<Boxes className="size-3.5" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">{t("figures.title")}</TooltipContent>
					</Tooltip>
				</TooltipProvider>
			</div>
			{showFigures ? (
				<aside className="agentero-scroll absolute inset-y-0 left-0 z-20 w-80 border-r bg-background/95 pt-11 pb-2 backdrop-blur-sm">
					<FiguresPanel
						documentId={documentId}
						viewerReady
						analyzing={analyzing}
						onAnalyze={onAnalyze}
						onJump={onJump}
						onRenderThumb={onRenderThumb}
						className="h-full"
					/>
				</aside>
			) : null}
		</>
	);
}
