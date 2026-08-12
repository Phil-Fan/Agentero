import { Check, FolderPlus } from "lucide-react";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { PopoverContent } from "@/components/ui/popover";
import { ScrollArea } from "@/components/ui/scroll-area";
import { usePapersOrgFolders } from "@/hooks/use-papers-org-folders";
import { normalizePath } from "@/lib/core/path";
import { cn } from "@/lib/core/utils";
import type { FileNode } from "@/lib/vault";

type MoveDestinationPickerProps = {
	vaultPath: string | null;
	nodes: FileNode[];
	sourcePaths: string[];
	selectedFolder: string;
	newFolder: string;
	busy: boolean;
	onNewFolderChange: (value: string) => void;
	onConfirm: (dest: string) => void | Promise<void>;
};

export function MoveDestinationPicker({
	vaultPath,
	nodes,
	sourcePaths,
	selectedFolder,
	newFolder,
	busy,
	onNewFolderChange,
	onConfirm,
}: MoveDestinationPickerProps) {
	const { t } = useTranslation("sidebar");
	const folders = usePapersOrgFolders(vaultPath, nodes, sourcePaths);
	const typed = newFolder.trim();
	const dest = typed
		? normalizePath(typed.startsWith("papers") ? typed : `papers/${typed}`)
		: selectedFolder;
	const destValid = dest.startsWith("papers");

	const handleSelect = (folder: string) => {
		onNewFolderChange("");
		void onConfirm(folder);
	};

	return (
		<PopoverContent
			align="start"
			side="right"
			sideOffset={4}
			avoidCollisions
			className="w-56 p-2"
		>
			<div className="space-y-2">
				<p className="font-medium text-xs text-foreground">
					{t("fileTree.moveToFolder", { count: sourcePaths.length })}
				</p>
				<ScrollArea className="h-40 rounded-md border">
					<div className="space-y-0.5 p-1">
						{folders.map((folder) => {
							const active = !typed && selectedFolder === folder;
							return (
								<button
									key={folder}
									type="button"
									disabled={busy}
									className={cn(
										"flex w-full items-center gap-2 rounded px-2 py-1 text-left text-xs transition-colors hover:bg-accent",
										active && "bg-muted",
									)}
									onClick={() => handleSelect(folder)}
								>
									<span className="flex-1 truncate font-mono">
										{folder === "papers"
											? t("fileTree.movePicker.papersRoot")
											: folder}
									</span>
									{active ? (
										<Check className="size-3 shrink-0 text-primary" />
									) : null}
								</button>
							);
						})}
					</div>
				</ScrollArea>
				<div className="space-y-1">
					<Label
						htmlFor="move-new-folder"
						className="text-[10px] text-muted-foreground"
					>
						{t("fileTree.movePicker.newFolder")}
					</Label>
					<div className="relative">
						<FolderPlus className="-translate-y-1/2 absolute top-1/2 left-2 size-3 text-muted-foreground" />
						<Input
							id="move-new-folder"
							value={newFolder}
							onChange={(e) => onNewFolderChange(e.target.value)}
							placeholder={t("fileTree.movePicker.newFolderHint")}
							disabled={busy}
							spellCheck={false}
							className="h-7 pl-6 text-xs font-mono"
							onKeyDown={(e) => {
								if (e.key === "Enter" && destValid) {
									e.preventDefault();
									void onConfirm(dest);
								}
							}}
						/>
					</div>
					{typed && !destValid ? (
						<p className="text-[10px] text-destructive">
							{t("fileTree.movePicker.invalidFolderPath")}
						</p>
					) : null}
				</div>
			</div>
		</PopoverContent>
	);
}
