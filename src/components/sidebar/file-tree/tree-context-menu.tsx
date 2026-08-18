import { Download, Loader2, Trash2 } from "lucide-react";
import { type RefObject, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { ViewportFloating } from "@/components/ui/viewport-floating";
import { LIBRARY_VIRTUAL_PATH, TRASH_VIRTUAL_PATH } from "@/lib/paper/api";
import { formatShortcutById } from "@/lib/shell/shortcuts";
import { revealInOsLabelKey } from "@/lib/vault/reveal";
import type { TreeContextMenu } from "./types";

export type TreeContextMenuPortalProps = {
	menu: TreeContextMenu;
	menuRef: RefObject<HTMLDivElement | null>;
	menuCount: number;
	menuNodeName?: string;
	isPaperMenu: boolean;
	libraryExportBusy: boolean;
	canPasteAtTarget: boolean;
	onClose: () => void;
	/** Each callback is optional — `undefined` hides the matching menu item. */
	onExportLibrary?: () => void;
	onEmptyTrash?: () => void;
	onOpenNotes?: () => void;
	/** Add the right-clicked file/paper to the Agent chat as a context chip. */
	onAddToChat?: () => void;
	onNewFile?: () => void;
	onNewFolder?: () => void;
	onCopyPath?: () => void;
	onCut?: () => void;
	onPaste?: () => void;
	onReveal?: () => void;
	onOpenInTerminal?: () => void;
	onMove?: () => void;
	onRename?: () => void;
	onDelete?: () => void;
};

export function TreeContextMenuPortal({
	menu,
	menuRef,
	menuCount,
	menuNodeName,
	isPaperMenu,
	libraryExportBusy,
	canPasteAtTarget,
	onClose,
	onExportLibrary,
	onEmptyTrash,
	onOpenNotes,
	onAddToChat,
	onNewFile,
	onNewFolder,
	onCopyPath,
	onCut,
	onPaste,
	onReveal,
	onOpenInTerminal,
	onMove,
	onRename,
	onDelete,
}: TreeContextMenuPortalProps) {
	const { t } = useTranslation("sidebar");

	// Re-register only when the menu opens/changes (matches original deps).
	// biome-ignore lint/correctness/useExhaustiveDependencies: intentional
	useEffect(() => {
		const close = () => onClose();
		const onKey = (e: KeyboardEvent) => {
			if (e.key === "Escape") close();
		};
		const onPointer = (e: PointerEvent) => {
			const el = menuRef.current;
			if (el && e.target instanceof Node && el.contains(e.target)) return;
			close();
		};
		// Defer so the opening contextmenu event does not immediately close.
		const timer = window.setTimeout(() => {
			window.addEventListener("pointerdown", onPointer, true);
			window.addEventListener("keydown", onKey, true);
			window.addEventListener("scroll", close, true);
			window.addEventListener("resize", close);
		}, 0);
		return () => {
			window.clearTimeout(timer);
			window.removeEventListener("pointerdown", onPointer, true);
			window.removeEventListener("keydown", onKey, true);
			window.removeEventListener("scroll", close, true);
			window.removeEventListener("resize", close);
		};
	}, [menu]);

	const isTrashMenu = menu.path === TRASH_VIRTUAL_PATH;
	const isLibraryMenu = menu.path === LIBRARY_VIRTUAL_PATH;

	const revealLabel = t(revealInOsLabelKey());
	const revealShortcut = formatShortcutById("revealInFinder");
	const openInTerminalShortcut = formatShortcutById("openInTerminal");
	const deleteShortcut = formatShortcutById("deleteTreeItem");
	const cutShortcut = formatShortcutById("cutTreeItem");
	const pasteShortcut = formatShortcutById("pasteTreeItem");

	return (
		<ViewportFloating
			floatingRef={menuRef}
			point={{ x: menu.x, y: menu.y }}
			role="menu"
			className="z-50 min-w-44 rounded-lg bg-popover p-1 text-popover-foreground shadow-md ring-1 ring-foreground/10"
		>
			{isLibraryMenu ? (
				onExportLibrary ? (
					<button
						type="button"
						role="menuitem"
						disabled={libraryExportBusy}
						className="flex w-full cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground disabled:pointer-events-none disabled:opacity-50"
						onClick={onExportLibrary}
					>
						{libraryExportBusy ? (
							<Loader2 className="size-3.5 shrink-0 animate-spin" aria-hidden />
						) : (
							<Download className="size-3.5 shrink-0" aria-hidden />
						)}
						<span>
							{libraryExportBusy
								? t("papersLibrary.exporting")
								: t("papersLibrary.export")}
						</span>
					</button>
				) : null
			) : isTrashMenu ? (
				onEmptyTrash ? (
					<button
						type="button"
						role="menuitem"
						className="flex w-full cursor-default items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm text-destructive outline-hidden select-none hover:bg-destructive/10 focus:bg-destructive/10"
						onClick={onEmptyTrash}
					>
						<Trash2 className="size-3.5 shrink-0" aria-hidden />
						<span>{t("recycleBin.emptyTrash")}</span>
					</button>
				) : null
			) : (
				<>
					{menuCount === 1 && isPaperMenu && onOpenNotes ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onOpenNotes}
						>
							<span>{t("fileTree.openNotes")}</span>
						</button>
					) : null}
					{menuCount === 1 && onAddToChat ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onAddToChat}
						>
							<span>{t("fileTree.addToChat")}</span>
						</button>
					) : null}
					{menuCount === 1 && onNewFile && !isPaperMenu ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onNewFile}
						>
							<span>{t("fileTree.newFile")}</span>
						</button>
					) : null}
					{menuCount === 1 && onNewFolder && !isPaperMenu ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onNewFolder}
						>
							<span>{t("fileTree.newFolder")}</span>
						</button>
					) : null}
					{menuCount === 1 && onCopyPath ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onCopyPath}
						>
							<span>{t("fileTree.copyPath")}</span>
						</button>
					) : null}
					{onCut ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center justify-between gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onCut}
						>
							<span>
								{menuCount > 1
									? t("fileTree.cutSelected", { count: menuCount })
									: t("fileTree.cut")}
							</span>
							<span className="text-muted-foreground text-xs tracking-wide">
								{cutShortcut}
							</span>
						</button>
					) : null}
					{canPasteAtTarget && onPaste ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center justify-between gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onPaste}
						>
							<span>
								{menuCount > 1 || isPaperMenu
									? t("fileTree.paste")
									: t("fileTree.pasteInto", {
											name: menuNodeName ?? t("fileTree.paste"),
										})}
							</span>
							<span className="text-muted-foreground text-xs tracking-wide">
								{pasteShortcut}
							</span>
						</button>
					) : null}
					{menuCount === 1 && onReveal ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center justify-between gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onReveal}
						>
							<span>{revealLabel}</span>
							<span className="text-muted-foreground text-xs tracking-wide">
								{revealShortcut}
							</span>
						</button>
					) : null}
					{menuCount === 1 && onOpenInTerminal ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center justify-between gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onOpenInTerminal}
						>
							<span>{t("fileTree.openInTerminal")}</span>
							<span className="text-muted-foreground text-xs tracking-wide">
								{openInTerminalShortcut}
							</span>
						</button>
					) : null}
					{onMove ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onMove}
						>
							<span>
								{menuCount > 1
									? t("fileTree.moveSelected", { count: menuCount })
									: t("fileTree.move")}
							</span>
						</button>
					) : null}
					{menuCount === 1 && onRename ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center gap-4 rounded-md px-2 py-1.5 text-left text-sm outline-hidden select-none hover:bg-accent hover:text-accent-foreground focus:bg-accent focus:text-accent-foreground"
							onClick={onRename}
						>
							<span>{t("fileTree.rename")}</span>
						</button>
					) : null}
					{onDelete ? (
						<button
							type="button"
							role="menuitem"
							className="flex w-full cursor-default items-center justify-between gap-4 rounded-md px-2 py-1.5 text-left text-sm text-destructive outline-hidden select-none hover:bg-destructive/10 focus:bg-destructive/10"
							onClick={onDelete}
						>
							<span>
								{menuCount > 1
									? t("fileTree.deleteSelected", { count: menuCount })
									: t("fileTree.delete")}
							</span>
							{menuCount === 1 ? (
								<span className="text-xs tracking-wide opacity-80">
									{deleteShortcut}
								</span>
							) : null}
						</button>
					) : null}
				</>
			)}
		</ViewportFloating>
	);
}
