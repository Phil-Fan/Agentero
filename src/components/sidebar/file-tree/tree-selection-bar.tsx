/** Header bar shown while rows are multi-selected: move / delete / clear. */
import { FolderInput, Trash2, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import {
	Tooltip,
	TooltipContent,
	TooltipTrigger,
} from "@/components/ui/tooltip";

export function TreeSelectionBar({
	count,
	onMove,
	onDelete,
	onClear,
}: {
	count: number;
	/** Undefined hides the move action; receives the button anchor in viewport coords. */
	onMove?: (anchor: { x: number; y: number }) => void;
	onDelete: () => void;
	onClear: () => void;
}) {
	const { t } = useTranslation("sidebar");
	return (
		<div className="mb-1 flex shrink-0 items-center gap-1 border-b bg-muted/95 px-3 py-1.5">
			<span className="text-muted-foreground text-xs">
				{t("fileTree.selectedCount", { count })}
			</span>
			<div className="ml-auto flex items-center gap-0.5">
				{onMove ? (
					<Tooltip>
						<TooltipTrigger asChild>
							<Button
								type="button"
								variant="ghost"
								size="icon-xs"
								className="size-6"
								aria-label={t("fileTree.moveSelected", { count })}
								onClick={(e) => {
									const rect = e.currentTarget.getBoundingClientRect();
									onMove({ x: rect.left, y: rect.top });
								}}
							>
								<FolderInput className="size-3.5" />
							</Button>
						</TooltipTrigger>
						<TooltipContent side="bottom">
							{t("fileTree.moveSelected", { count })}
						</TooltipContent>
					</Tooltip>
				) : null}
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							className="size-6 text-destructive"
							aria-label={t("fileTree.deleteSelected", { count })}
							onClick={onDelete}
						>
							<Trash2 className="size-3.5" />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom">
						{t("fileTree.deleteSelected", { count })}
					</TooltipContent>
				</Tooltip>
				<Tooltip>
					<TooltipTrigger asChild>
						<Button
							type="button"
							variant="ghost"
							size="icon-xs"
							className="size-6"
							aria-label={t("fileTree.clearSelection")}
							onClick={onClear}
						>
							<X className="size-3.5" />
						</Button>
					</TooltipTrigger>
					<TooltipContent side="bottom">
						{t("fileTree.clearSelection")}
					</TooltipContent>
				</Tooltip>
			</div>
		</div>
	);
}
