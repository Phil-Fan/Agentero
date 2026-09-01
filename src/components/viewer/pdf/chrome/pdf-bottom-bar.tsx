import { Moon, MoveHorizontal, MoveVertical, Sun } from "lucide-react";
import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import { SiArxiv } from "react-icons/si";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipProvider,
	TooltipTrigger,
} from "@/components/ui/tooltip";
import { cn } from "@/lib/core/utils";

type PdfBottomBarProps = {
	/** Hidden until the document reports its page count. */
	totalPages: number;
	/** Editable page number (raw digits while typing). */
	pageField: string;
	onPageFieldChange: (value: string) => void;
	/** True while the field owns focus, so scrolling does not clobber typing. */
	pageFocusedRef: RefObject<boolean>;
	onCommitPageField: () => void;
	pdfDark: boolean;
	onTogglePdfColorScheme: () => void;
	/** Request the fit-width zoom mode. */
	onFitWidth: () => void;
	/** Request the fit-page zoom mode. */
	onFitPage: () => void;
	/** True for remote arXiv papers with no local sidecar. */
	isRemotePaper?: boolean;
};

/** Bottom bar: page nav + PDF color scheme. */
export function PdfBottomBar({
	totalPages,
	pageField,
	onPageFieldChange,
	pageFocusedRef,
	onCommitPageField,
	pdfDark,
	onTogglePdfColorScheme,
	onFitWidth,
	onFitPage,
	isRemotePaper = false,
}: PdfBottomBarProps) {
	const { t } = useTranslation("viewer");

	if (totalPages <= 0) return null;

	const pdfColorSchemeLabel = pdfDark
		? t("pdf.useLightMode")
		: t("pdf.useDarkMode");

	return (
		<div className="pointer-events-none absolute bottom-3 left-1/2 z-20 -translate-x-1/2">
			<TooltipProvider delayDuration={200}>
				<div className="pointer-events-auto flex items-center gap-0.5 rounded-lg border border-border/80 bg-background/95 p-0.5 shadow-sm backdrop-blur-sm">
					{isRemotePaper ? (
						<Tooltip>
							<TooltipTrigger asChild>
								<div
									className={cn(
										"flex items-center gap-1 rounded px-1.5 py-0.5 text-primary",
										"bg-primary/10",
									)}
								>
									<SiArxiv
										className="size-3.5 shrink-0 text-[#B31B1B]"
										aria-hidden
									/>
									<span className="whitespace-nowrap text-[11px] font-medium">
										{t("pdf.remoteMode")}
									</span>
								</div>
							</TooltipTrigger>
							<TooltipContent side="top">{t("pdf.remoteMode")}</TooltipContent>
						</Tooltip>
					) : null}
					<input
						type="text"
						inputMode="numeric"
						className="w-6 rounded bg-transparent text-center font-medium text-foreground text-xs tabular-nums outline-none focus:bg-muted"
						aria-label={t("pdf.goToPage")}
						value={pageField}
						onFocus={(e) => {
							pageFocusedRef.current = true;
							e.currentTarget.select();
						}}
						onChange={(e) =>
							onPageFieldChange(e.target.value.replace(/[^0-9]/g, ""))
						}
						onKeyDown={(e) => {
							if (e.key === "Enter") {
								e.preventDefault();
								onCommitPageField();
								e.currentTarget.blur();
							}
						}}
						onBlur={() => {
							pageFocusedRef.current = false;
							onCommitPageField();
						}}
					/>
					<span className="px-0.5 text-muted-foreground text-xs tabular-nums">
						/ {totalPages}
					</span>
					<span aria-hidden className="mx-0.5 h-3.5 w-px shrink-0 bg-border" />
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								size="icon-xs"
								variant="ghost"
								aria-label={pdfColorSchemeLabel}
								aria-pressed={pdfDark}
								onClick={onTogglePdfColorScheme}
							>
								{pdfDark ? (
									<Sun className="size-3.5" />
								) : (
									<Moon className="size-3.5" />
								)}
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{pdfColorSchemeLabel}</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								size="icon-xs"
								variant="ghost"
								aria-label={t("pdf.zoomFit")}
								onClick={onFitWidth}
							>
								<MoveHorizontal className="size-3.5" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{t("pdf.zoomFit")}</TooltipContent>
					</Tooltip>
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								size="icon-xs"
								variant="ghost"
								aria-label={t("pdf.zoomFitPage")}
								onClick={onFitPage}
							>
								<MoveVertical className="size-3.5" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="top">{t("pdf.zoomFitPage")}</TooltipContent>
					</Tooltip>
				</div>
			</TooltipProvider>
		</div>
	);
}
