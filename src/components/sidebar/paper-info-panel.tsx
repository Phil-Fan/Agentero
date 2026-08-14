import {
	BookOpen,
	Calendar,
	ChevronRight,
	ExternalLink,
	Info,
	Tag,
	Users,
} from "lucide-react";
import {
	type ComponentType,
	type KeyboardEvent,
	type ReactNode,
	type PointerEvent as ReactPointerEvent,
	useEffect,
	useRef,
	useState,
} from "react";
import { useTranslation } from "react-i18next";
import { SiArxiv, SiModelscope } from "react-icons/si";

import {
	PaperTagChip,
	PaperTagRemoveButton,
} from "@/components/library/paper-tag-chip";
import {
	Collapsible,
	CollapsibleContent,
	CollapsibleTrigger,
} from "@/components/ui/collapsible";
import { Input } from "@/components/ui/input";
import {
	Popover,
	PopoverContent,
	PopoverTrigger,
} from "@/components/ui/popover";
import { copyTextToClipboard } from "@/lib/core/clipboard";
import { cn } from "@/lib/core/utils";
import type { PaperMetadata } from "@/lib/paper";
import { arxivUrls } from "@/lib/paper/arxiv";
import {
	coercePaperTags,
	isConnectorTagName,
	normalizePaperTags,
	type PaperTag,
	TAG_COLOR_IDS,
	type TagColorId,
	tagSwatchStyle,
	visiblePaperTags,
} from "@/lib/ui/tag-colors";

type PaperInfoPanelProps = {
	meta: PaperMetadata | null;
	className?: string;
	/** Persist tags to catalog. Required for editing. */
	onTagsChange?: (tags: PaperTag[]) => Promise<void> | void;
};

function MetaRow({
	icon: Icon,
	label,
	children,
}: {
	icon: ComponentType<{ className?: string }>;
	label: string;
	children: ReactNode;
}) {
	return (
		<div className="flex gap-2 px-3 py-1.5">
			<Icon
				className="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
				aria-hidden
			/>
			<div className="min-w-0 flex-1">
				<div className="text-[10px] text-muted-foreground uppercase tracking-wide leading-none">
					{label}
				</div>
				<div className="mt-0.5 text-xs leading-snug text-foreground">
					{children}
				</div>
			</div>
		</div>
	);
}

/** Single-click-to-copy value control, same interaction as Library cells. */
function CopyValue({
	text,
	label,
	className,
	onCopy,
}: {
	text: string;
	label: string;
	className?: string;
	onCopy: (text: string, label: string) => void;
}) {
	const { t } = useTranslation("sidebar");
	const hint = t("paperInfo.copyHint", { label });
	return (
		<button
			type="button"
			title={hint}
			aria-label={hint}
			onClick={() => onCopy(text, label)}
			className={cn(
				"block w-full cursor-pointer rounded-sm text-left",
				"hover:bg-muted/60 hover:text-foreground",
				"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
			)}
		>
			{/* No `block` here: it sorts after line-clamp-* and would override display:-webkit-box. */}
			<span className={cn("w-full", className)}>{text}</span>
		</button>
	);
}

function LinkChip({
	href,
	label,
	icon,
}: {
	href: string;
	label: string;
	icon?: ReactNode;
}) {
	return (
		<a
			href={href}
			target="_blank"
			rel="noreferrer"
			className={cn(
				"inline-flex items-center gap-1 rounded-md border bg-background px-1.5 py-0.5",
				"text-[11px] text-muted-foreground transition-colors",
				"hover:bg-muted hover:text-foreground",
			)}
		>
			{icon}
			{label}
			<ExternalLink className="size-2.5 opacity-70" aria-hidden />
		</a>
	);
}

function ServiceLinkChip({
	href,
	label,
	icon,
}: {
	href: string;
	label: string;
	icon: ReactNode;
}) {
	return (
		<a
			href={href}
			target="_blank"
			rel="noreferrer"
			aria-label={label}
			title={label}
			className={cn(
				"inline-flex h-6 min-w-0 max-w-full items-center gap-1.5 rounded-md border bg-background px-2",
				"text-[11px] text-muted-foreground transition-colors",
				"hover:bg-muted hover:text-foreground",
				"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
			)}
		>
			{icon}
			<span className="min-w-0 truncate">{label}</span>
			<ExternalLink className="size-2.5 shrink-0 opacity-70" aria-hidden />
		</a>
	);
}

function TagsEditor({
	tags,
	disabled,
	onChange,
}: {
	tags: PaperTag[] | unknown;
	disabled?: boolean;
	onChange: (tags: PaperTag[]) => void;
}) {
	const { t } = useTranslation("sidebar");
	const [draft, setDraft] = useState("");
	const [draftColor, setDraftColor] = useState<TagColorId | null>(null);
	const [colorOpen, setColorOpen] = useState(false);
	const [busy, setBusy] = useState(false);
	const inputRef = useRef<HTMLInputElement>(null);
	const allTags = coercePaperTags(tags);
	const list = visiblePaperTags(allTags);

	const commit = async (next: PaperTag[]) => {
		const hidden = allTags.filter((tag) => isConnectorTagName(tag.name));
		const normalized = normalizePaperTags([...hidden, ...next]);
		setBusy(true);
		try {
			await onChange(normalized);
		} finally {
			setBusy(false);
		}
	};

	const addTag = () => {
		const value = draft.trim();
		if (!value || busy || disabled) return;
		setDraft("");
		const next: PaperTag = draftColor
			? { name: value, color: draftColor }
			: { name: value };
		// Keep selected color for consecutive tags of the same theme.
		void commit([...list, next]);
	};

	const removeTag = (name: string) => {
		if (busy || disabled) return;
		void commit(
			list.filter(
				(x) => x.name.toLocaleLowerCase() !== name.toLocaleLowerCase(),
			),
		);
	};

	const onKeyDown = (e: KeyboardEvent<HTMLInputElement>) => {
		if (e.key === "Enter") {
			e.preventDefault();
			addTag();
		} else if (e.key === "Backspace" && !draft && list.length > 0) {
			const last = list[list.length - 1];
			if (last) removeTag(last.name);
		}
	};

	const draftSwatch = tagSwatchStyle(draftColor);

	return (
		<div className="flex flex-col gap-1.5">
			{/* Input first so Zotero papers with many imported tags still show "Add tag…" without scrolling past chips. */}
			{disabled ? null : (
				<div className="relative">
					<Input
						ref={inputRef}
						value={draft}
						onChange={(e) => setDraft(e.target.value)}
						onKeyDown={onKeyDown}
						placeholder={t("paperInfo.addTag")}
						aria-label={t("paperInfo.addTag")}
						disabled={busy}
						className="h-6 border-dashed py-0 pr-7 pl-1.5 text-[11px]"
					/>
					<Popover open={colorOpen} onOpenChange={setColorOpen}>
						<PopoverTrigger asChild>
							<button
								type="button"
								data-tag-color-picker
								disabled={busy}
								className={cn(
									"absolute top-1/2 right-1 flex size-4 -translate-y-1/2 items-center justify-center",
									"overflow-hidden rounded-full bg-background ring-1 ring-border/70 transition-colors",
									"hover:ring-foreground/30",
									"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
									"disabled:pointer-events-none disabled:opacity-50",
								)}
								style={draftSwatch}
								aria-label={t("paperInfo.tagColor")}
								title={t("paperInfo.tagColor")}
							>
								{draftColor == null ? (
									<span
										className="pointer-events-none absolute top-1/2 left-[-20%] h-px w-[140%] -translate-y-1/2 rotate-45 bg-red-500"
										aria-hidden
									/>
								) : null}
								<span className="sr-only">{t("paperInfo.tagColor")}</span>
							</button>
						</PopoverTrigger>
						<PopoverContent
							data-tag-color-picker
							side="top"
							align="end"
							sideOffset={6}
							className="w-auto p-2"
							onOpenAutoFocus={(e) => e.preventDefault()}
						>
							<div className="flex items-center gap-1.5">
								<button
									type="button"
									data-tag-color-picker
									className={cn(
										"relative size-5 overflow-hidden rounded-full bg-background ring-1 ring-border transition-shadow",
										"hover:ring-foreground/40",
										"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
										draftColor == null && "ring-2 ring-foreground/50",
									)}
									aria-label={t("paperInfo.tagColorDefault")}
									title={t("paperInfo.tagColorDefault")}
									onClick={() => {
										setDraftColor(null);
										setColorOpen(false);
										inputRef.current?.focus();
									}}
								>
									<span
										className="pointer-events-none absolute top-1/2 left-[-20%] h-px w-[140%] -translate-y-1/2 rotate-45 bg-red-500"
										aria-hidden
									/>
								</button>
								{TAG_COLOR_IDS.map((id) => {
									const style = tagSwatchStyle(id);
									const selected = draftColor === id;
									return (
										<button
											key={id}
											type="button"
											data-tag-color-picker
											className={cn(
												"size-5 rounded-full ring-1 ring-black/10 transition-shadow",
												"hover:ring-foreground/40",
												"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
												selected && "ring-2 ring-foreground/50",
											)}
											style={style}
											aria-label={t("paperInfo.tagColorNamed", { color: id })}
											title={id}
											onClick={() => {
												setDraftColor(id);
												setColorOpen(false);
												inputRef.current?.focus();
											}}
										/>
									);
								})}
							</div>
						</PopoverContent>
					</Popover>
				</div>
			)}
			{list.length > 0 ? (
				<div className="flex flex-wrap gap-1">
					{list.map((tag) => (
						<PaperTagChip
							key={tag.name}
							tag={tag}
							trailing={
								disabled ? null : (
									<PaperTagRemoveButton
										tagName={tag.name}
										label={t("paperInfo.removeTag", { tag: tag.name })}
										disabled={busy}
										onRemove={removeTag}
									/>
								)
							}
						/>
					))}
				</div>
			) : null}
		</div>
	);
}

const HEIGHT_STORAGE_KEY = "agentero.paperInfoHeight";
const MIN_CONTENT_HEIGHT = 120;
const DEFAULT_CONTENT_HEIGHT = 320;
const HEADER_HEIGHT = 32;

function clampHeight(value: number) {
	return Math.max(MIN_CONTENT_HEIGHT, value);
}

function loadStoredHeight(): number {
	try {
		const raw = localStorage.getItem(HEIGHT_STORAGE_KEY);
		const parsed = raw ? Number.parseInt(raw, 10) : Number.NaN;
		if (Number.isFinite(parsed)) return clampHeight(parsed);
	} catch {
		// localStorage unavailable; fall through to default.
	}
	return DEFAULT_CONTENT_HEIGHT;
}

export function PaperInfoPanel({
	meta,
	className,
	onTagsChange,
}: PaperInfoPanelProps) {
	const { t } = useTranslation("sidebar");
	const [open, setOpen] = useState(Boolean(meta));
	const [contentHeight, setContentHeight] = useState(loadStoredHeight);
	const dragRef = useRef<{
		startY: number;
		startHeight: number;
		collapseOnRelease: boolean;
	} | null>(null);

	const persistHeight = (value: number) => {
		try {
			localStorage.setItem(HEIGHT_STORAGE_KEY, String(Math.round(value)));
		} catch {
			// Ignore persistence failures.
		}
	};

	const onHandlePointerDown = (e: ReactPointerEvent<HTMLDivElement>) => {
		if (e.button !== 0) return;
		dragRef.current = {
			startY: e.clientY,
			startHeight: contentHeight,
			collapseOnRelease: false,
		};
		e.currentTarget.setPointerCapture(e.pointerId);
	};

	const onHandlePointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
		const drag = dragRef.current;
		if (!drag) return;
		// Dragging up grows the panel.
		const next = drag.startHeight + (drag.startY - e.clientY);
		drag.collapseOnRelease = next <= MIN_CONTENT_HEIGHT;
		setContentHeight(clampHeight(next));
	};

	const onHandlePointerUp = (e: ReactPointerEvent<HTMLDivElement>) => {
		const drag = dragRef.current;
		if (!drag) return;
		dragRef.current = null;
		e.currentTarget.releasePointerCapture(e.pointerId);
		setContentHeight((h) => {
			persistHeight(h);
			return h;
		});
		if (drag.collapseOnRelease) setOpen(false);
	};

	const onHandleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
		const step = e.shiftKey ? 48 : 16;
		let next: number | null = null;
		if (e.key === "ArrowUp") next = clampHeight(contentHeight + step);
		else if (e.key === "ArrowDown") next = clampHeight(contentHeight - step);
		if (next == null) return;
		e.preventDefault();
		if (e.key === "ArrowDown" && contentHeight <= MIN_CONTENT_HEIGHT) {
			setOpen(false);
			return;
		}
		setContentHeight(next);
		persistHeight(next);
	};

	// Open when a paper is selected; collapse when none.
	useEffect(() => {
		if (!meta) {
			setOpen(false);
			return;
		}
		setOpen(true);
	}, [meta]);

	const resizable = open && Boolean(meta);
	const arxivId = meta?.arxiv_id ? arxivUrls(meta.arxiv_id)?.id : null;
	const modelScopeUrl = arxivId
		? `https://modelscope.cn/papers/${arxivId}/overview`
		: null;
	const alphaXivUrl = arxivId
		? `https://www.alphaxiv.org/abs/${arxivId}`
		: null;

	const copyField = async (text: string | null | undefined, label: string) => {
		const value = text?.trim();
		if (!value) return;
		await copyTextToClipboard(value, {
			successMessage: t("paperInfo.copied", { label }),
			errorMessage: t("paperInfo.copyFailed"),
			successNotify: {
				duration: 1500,
				id: "paper-info-copied",
			},
		});
	};

	return (
		<div
			className={cn(
				"relative flex min-h-0 shrink-0 flex-col border-t",
				className,
			)}
			style={{ height: open ? contentHeight + HEADER_HEIGHT : HEADER_HEIGHT }}
		>
			{resizable ? (
				// biome-ignore lint/a11y/useSemanticElements: a focusable drag separator cannot be a native <hr>
				<div
					role="separator"
					aria-orientation="horizontal"
					aria-label={t("paperInfo.resize")}
					aria-valuenow={Math.round(contentHeight)}
					aria-valuemin={MIN_CONTENT_HEIGHT}
					title={t("paperInfo.resize")}
					tabIndex={0}
					onPointerDown={onHandlePointerDown}
					onPointerMove={onHandlePointerMove}
					onPointerUp={onHandlePointerUp}
					onPointerCancel={onHandlePointerUp}
					onKeyDown={onHandleKeyDown}
					className={cn(
						// Sits on the panel's top border; wider invisible hit area.
						"absolute inset-x-0 -top-[3px] z-10 h-[7px] cursor-row-resize outline-none",
						"after:absolute after:inset-x-0 after:top-[3px] after:h-px after:content-['']",
						"hover:after:bg-foreground/25 focus-visible:after:bg-foreground/35",
					)}
				/>
			) : null}
			<Collapsible
				open={open}
				onOpenChange={setOpen}
				className="flex min-h-0 flex-1 flex-col"
			>
				<div className="flex h-8 min-h-8 shrink-0 items-center pr-1.5">
					<CollapsibleTrigger
						className={cn(
							"flex min-w-0 flex-1 items-center gap-1.5 px-2 text-left outline-none",
							"text-muted-foreground text-xs font-medium tracking-wide",
							"hover:bg-muted/40 hover:text-foreground",
							"focus-visible:ring-1 focus-visible:ring-ring",
						)}
					>
						<ChevronRight
							className={cn(
								"size-3.5 shrink-0 transition-transform",
								open && "rotate-90",
							)}
							aria-hidden
						/>
						<Info className="size-3.5 shrink-0" aria-hidden />
						<span className="truncate">{t("paperInfo.info")}</span>
					</CollapsibleTrigger>
					{arxivId ? (
						<button
							type="button"
							title={t("paperInfo.copyHint", {
								label: t("paperInfo.arxivId"),
							})}
							aria-label={t("paperInfo.copyHint", {
								label: t("paperInfo.arxivId"),
							})}
							onClick={() => void copyField(arxivId, t("paperInfo.arxivId"))}
							className={cn(
								"flex min-w-0 max-w-[55%] shrink cursor-pointer items-center gap-1 rounded-md px-1.5 py-0.5",
								"text-[10px] text-muted-foreground tabular-nums transition-colors",
								"hover:bg-muted hover:text-foreground",
								"focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring",
							)}
						>
							<SiArxiv className="size-3 shrink-0" aria-hidden />
							<span className="truncate">{arxivId}</span>
						</button>
					) : null}
				</div>
				<CollapsibleContent className="flex min-h-0 flex-1 flex-col">
					{!meta ? (
						<p className="px-3 pb-3 text-muted-foreground text-xs leading-snug">
							{t("paperInfo.selectPrompt")}
						</p>
					) : (
						<div className="agentero-scroll min-h-0 flex-1 overflow-y-auto pb-2">
							<MetaRow icon={BookOpen} label={t("paperInfo.title")}>
								<CopyValue
									text={meta.title}
									label={t("paperInfo.title")}
									onCopy={copyField}
									className="line-clamp-2 font-medium"
								/>
							</MetaRow>
							{meta.authors?.length ? (
								<MetaRow icon={Users} label={t("paperInfo.authors")}>
									<CopyValue
										text={meta.authors.join(", ")}
										label={t("paperInfo.authors")}
										onCopy={copyField}
										className="line-clamp-2"
									/>
								</MetaRow>
							) : null}
							{meta.year ? (
								<MetaRow icon={Calendar} label={t("paperInfo.year")}>
									{meta.year}
								</MetaRow>
							) : null}
							<MetaRow icon={Tag} label={t("paperInfo.tags")}>
								<TagsEditor
									tags={meta.tags ?? []}
									// Editable whenever parent can persist; path is resolved in App
									// Prefer catalog path; else open paper folder.
									disabled={!onTagsChange}
									onChange={async (tags) => {
										if (onTagsChange) await onTagsChange(tags);
									}}
								/>
							</MetaRow>
							{(meta.pdf_url || meta.source_url || meta.arxiv_id) && (
								<div className="flex flex-wrap gap-1.5 px-3 pt-1">
									{meta.pdf_url || meta.arxiv_id ? (
										<LinkChip
											href={
												meta.pdf_url ?? `https://arxiv.org/pdf/${meta.arxiv_id}`
											}
											label={t("paperInfo.pdf")}
										/>
									) : null}
									{meta.source_url || meta.arxiv_id ? (
										<LinkChip
											href={
												meta.source_url ??
												`https://arxiv.org/abs/${meta.arxiv_id}`
											}
											label={t("paperInfo.abs")}
										/>
									) : null}
								</div>
							)}
							{modelScopeUrl || alphaXivUrl ? (
								<div className="flex flex-wrap gap-1.5 px-3 pt-1.5">
									{modelScopeUrl ? (
										<ServiceLinkChip
											href={modelScopeUrl}
											label={t("paperInfo.modelscopeInterpretation")}
											icon={
												<SiModelscope className="size-3.5 shrink-0 text-[#624AFF]" />
											}
										/>
									) : null}
									{alphaXivUrl ? (
										<ServiceLinkChip
											href={alphaXivUrl}
											label={t("paperInfo.alphaXiv")}
											icon={
												<SiArxiv className="size-3.5 shrink-0 text-[#B31B1B]" />
											}
										/>
									) : null}
								</div>
							) : null}
						</div>
					)}
				</CollapsibleContent>
			</Collapsible>
		</div>
	);
}
