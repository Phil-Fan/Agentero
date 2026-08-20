import { Check, SearchCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Shimmer } from "@/components/ai-elements/shimmer";
import { Button } from "@/components/ui/button";
import {
	Dialog,
	DialogContent,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { useVaultStore } from "@/hooks/use-app-stores";
import { useOverlayRegistration } from "@/hooks/use-overlay-registration";
import { usePapersOrgFolders } from "@/hooks/use-papers-org-folders";
import { basenameOf } from "@/lib/core/path";
import { cn } from "@/lib/core/utils";
import {
	type PdfIdentProbe,
	probePdfIdents,
	resolveIdentifierMetadata,
} from "@/lib/paper/api";
import type {
	LocalPdfExtraMeta,
	LocalPdfImportEntry,
} from "@/lib/paper/lookup";

type DraftRow = {
	filePath: string;
	/** Original filename for UI (not the staging path). */
	sourceName: string;
	title: string;
	authors: string;
	year: string;
	/** Single DOI or arXiv ID (or URL) input; classified on submit. */
	identifier: string;
	/** Structured fields from probe/Fetch, submitted as `extra`. */
	extra?: LocalPdfExtraMeta;
};

export type ImportLocalPdfDraftItem = {
	path: string;
	sourceName: string;
};

function identifierKind(text: string): "DOI" | "arXiv" | null {
	const v = text.trim();
	if (!v) return null;
	if (/^10\.\d{4,}\//.test(v) || /doi\.org\//i.test(v)) return "DOI";
	return "arXiv";
}

function draftsFromItems(items: ImportLocalPdfDraftItem[]): DraftRow[] {
	return items.map((item) => {
		return {
			filePath: item.path,
			sourceName: item.sourceName || basenameOf(item.path),
			title: "",
			authors: "",
			year: "",
			identifier: "",
		};
	});
}

function applyProbe(row: DraftRow, probe: PdfIdentProbe): DraftRow {
	if (probe.status === "no-match" || probe.status === "error") return row;
	const next: DraftRow = { ...row };
	if (probe.title?.trim()) next.title = probe.title.trim();
	if (probe.authors.length) next.authors = probe.authors.join(", ");
	if (probe.year != null) next.year = String(probe.year);
	const ident = probe.doi?.trim() || probe.arxivId?.trim();
	if (ident) next.identifier = ident;
	next.extra = {
		publication: probe.publication,
		volume: probe.volume,
		issue: probe.issue,
		pages: probe.pages,
		publisher: probe.publisher,
		abstract: probe.abstractText,
	};
	return next;
}

/**
 * Confirm parent folder + per-PDF metadata before `paper_import_local_pdf`.
 * Opened when the user drops PDFs onto a `papers/` org folder or the Library.
 * On open, each PDF is recognized (DOI/arXiv/title) to prefill the rows.
 */
export function ImportLocalPdfDialog({
	open,
	onOpenChange,
	items,
	parentDir,
	onConfirm,
	busy,
}: {
	open: boolean;
	onOpenChange: (open: boolean) => void;
	items: ImportLocalPdfDraftItem[];
	/** Vault-relative drop target, e.g. `papers` or `papers/nlp`. */
	parentDir: string;
	onConfirm: (entries: LocalPdfImportEntry[], parentDir: string) => void;
	busy?: boolean;
}) {
	const { t } = useTranslation("sidebar");
	const [rows, setRows] = useState<DraftRow[]>([]);
	const [dest, setDest] = useState(parentDir);
	const [destOpen, setDestOpen] = useState(false);
	const vaultPath = useVaultStore((s) => s.vaultPath);
	const tree = useVaultStore((s) => s.tree);
	const folders = usePapersOrgFolders(vaultPath, tree);
	const destMatches = useMemo(() => {
		const q = dest.trim().toLowerCase();
		return folders.filter((f) => !q || f.toLowerCase().includes(q));
	}, [folders, dest]);
	const [probing, setProbing] = useState(false);
	const [probeStatus, setProbeStatus] = useState<Record<string, string>>({});
	const [fetching, setFetching] = useState<Record<string, boolean>>({});
	const [dots, setDots] = useState(".");
	useEffect(() => {
		if (!probing) {
			setDots(".");
			return;
		}
		const id = setInterval(
			() => setDots((d) => (d.length >= 3 ? "." : `${d}.`)),
			400,
		);
		return () => clearInterval(id);
	}, [probing]);
	const probingPlaceholder = probing ? dots : undefined;

	useOverlayRegistration("import-local-pdf", open, () => onOpenChange(false));

	useEffect(() => {
		if (!open) return;
		setRows(draftsFromItems(items));
		setDest(parentDir || "papers");
		setProbeStatus({});
		setFetching({});
		// Best-effort recognition: failures leave empty fields for manual entry.
		const paths = items.map((i) => i.path);
		if (!paths.length) return;
		setProbing(true);
		probePdfIdents(paths)
			.then((probes) => {
				setRows((prev) =>
					prev.map((row) => {
						const probe = probes.find((p) => p.filePath === row.filePath);
						return probe ? applyProbe(row, probe) : row;
					}),
				);
				setProbeStatus(
					Object.fromEntries(probes.map((p) => [p.filePath, p.status])),
				);
			})
			.catch(() => {
				/* leave fields empty for manual entry */
			})
			.finally(() => setProbing(false));
	}, [open, items, parentDir]);

	const canSubmit = useMemo(() => {
		if (!rows.length || busy || probing) return false;
		return rows.every((r) => r.title.trim().length > 0);
	}, [rows, busy, probing]);

	const updateRow = (index: number, patch: Partial<DraftRow>) => {
		setRows((prev) =>
			prev.map((r, i) => (i === index ? { ...r, ...patch } : r)),
		);
	};

	const handleFetch = async (index: number, row: DraftRow) => {
		const text = row.identifier.trim();
		if (!text) return;
		setFetching((prev) => ({ ...prev, [row.filePath]: true }));
		try {
			const meta = await resolveIdentifierMetadata(text);
			setRows((prev) =>
				prev.map((r, i) => {
					if (i !== index) return r;
					const next: DraftRow = { ...r };
					if (meta.title?.trim()) next.title = meta.title.trim();
					if (meta.authors?.length) next.authors = meta.authors.join(", ");
					if (meta.year != null) next.year = String(meta.year);
					const ident = meta.doi?.trim() || meta.arxivId?.trim();
					if (ident) next.identifier = ident;
					next.extra = {
						publication: meta.publication,
						volume: meta.volume,
						issue: meta.issue,
						pages: meta.pages,
						publisher: meta.publisher,
						date: meta.date,
						abstract: meta.abstract,
					};
					return next;
				}),
			);
			setProbeStatus((prev) => ({ ...prev, [row.filePath]: "ok" }));
		} catch {
			setProbeStatus((prev) => ({ ...prev, [row.filePath]: "error" }));
		} finally {
			setFetching((prev) => ({ ...prev, [row.filePath]: false }));
		}
	};

	const handleConfirm = () => {
		if (!canSubmit) return;
		const entries: LocalPdfImportEntry[] = rows.map((r) => {
			const authors = r.authors
				.split(/[,;，；]/)
				.map((a) => a.trim())
				.filter(Boolean);
			const yearRaw = r.year.trim();
			const yearNum = yearRaw ? Number.parseInt(yearRaw, 10) : NaN;
			const ident = r.identifier.trim();
			const kind = identifierKind(ident);
			return {
				filePath: r.filePath,
				title: r.title.trim(),
				authors: authors.length ? authors : undefined,
				year: Number.isFinite(yearNum) ? yearNum : undefined,
				doi: kind === "DOI" ? ident : undefined,
				arxivId: kind === "arXiv" ? ident : undefined,
				extra: ident ? r.extra : undefined,
			};
		});
		onConfirm(entries, dest.trim() || "papers");
	};

	const headerAction = (row: DraftRow, index: number) => {
		if (probing) {
			return (
				<span className="text-muted-foreground text-xs">
					{t("importLocalPdf.recognizing")}
				</span>
			);
		}
		if (fetching[row.filePath]) {
			return (
				<Shimmer as="span" className="text-xs">
					{t("importLocalPdf.fetching")}
				</Shimmer>
			);
		}
		const status = probeStatus[row.filePath];
		if (status === "ok" || status === "title") {
			return (
				<span className="inline-flex items-center gap-1 text-xs text-emerald-600 dark:text-emerald-400">
					<SearchCheck className="size-3" aria-hidden />
					{t("importLocalPdf.recognized")}
				</span>
			);
		}
		return (
			<Button
				type="button"
				variant="outline"
				size="sm"
				className="h-7 px-2 text-xs"
				disabled={busy || !row.identifier.trim()}
				onClick={() => void handleFetch(index, row)}
			>
				{t("importLocalPdf.fetch")}
			</Button>
		);
	};

	return (
		<Dialog open={open} onOpenChange={onOpenChange}>
			<DialogContent
				className="flex! max-h-[70vh] min-h-0 flex-col gap-3 overflow-hidden sm:max-w-lg"
				aria-describedby={undefined}
			>
				<DialogHeader className="shrink-0">
					<DialogTitle>
						{t("importLocalPdf.title", { count: items.length })}
					</DialogTitle>
				</DialogHeader>

				<div className="relative shrink-0 space-y-1.5">
					<Label htmlFor="import-pdf-parent" className="text-xs">
						{t("importLocalPdf.parentDir")}
					</Label>
					<Input
						id="import-pdf-parent"
						value={dest}
						onChange={(e) => {
							setDest(e.target.value);
							setDestOpen(true);
						}}
						onFocus={() => setDestOpen(true)}
						onBlur={() => setDestOpen(false)}
						disabled={busy}
						spellCheck={false}
						className="font-mono text-xs"
					/>
					{destOpen && destMatches.length > 0 ? (
						<div className="absolute inset-x-0 top-full z-10 mt-1 max-h-40 overflow-y-auto rounded-md border bg-popover p-1 shadow-md">
							{destMatches.map((folder) => {
								const active = folder === dest.trim();
								return (
									<button
										key={folder}
										type="button"
										className={cn(
											"flex w-full items-center gap-2 rounded px-2 py-1 text-left font-mono text-xs transition-colors hover:bg-accent",
											active && "bg-muted",
										)}
										onMouseDown={(e) => {
											e.preventDefault();
											setDest(folder);
											setDestOpen(false);
										}}
									>
										<span className="flex-1 truncate">
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
					) : null}
				</div>

				<ul className="min-h-0 flex-1 list-none space-y-2 overflow-y-auto overscroll-contain pr-1">
					{rows.map((row, index) => (
						<li
							key={row.filePath}
							className="space-y-2 rounded-lg border border-border/80 p-2.5"
						>
							<div className="flex items-center justify-between gap-2">
								<p
									className="truncate font-medium text-muted-foreground text-xs"
									title={row.filePath}
								>
									{row.sourceName || basenameOf(row.filePath)}
								</p>
								{headerAction(row, index)}
							</div>
							<div className="flex items-center gap-2">
								<Label className="w-16 shrink-0 text-xs">
									{t("importLocalPdf.fieldTitle")}
								</Label>
								<Input
									value={row.title}
									onChange={(e) => updateRow(index, { title: e.target.value })}
									placeholder={probingPlaceholder}
									disabled={busy}
								/>
							</div>
							<div className="flex items-center gap-2">
								<Label className="w-16 shrink-0 text-xs">
									{t("importLocalPdf.fieldAuthors")}
								</Label>
								<Input
									value={row.authors}
									onChange={(e) =>
										updateRow(index, { authors: e.target.value })
									}
									placeholder={probingPlaceholder}
									disabled={busy}
								/>
							</div>
							<div className="flex items-center gap-2">
								<Label className="w-16 shrink-0 text-xs">
									{t("importLocalPdf.fieldIdentifier")}
								</Label>
								<div className="relative min-w-0 flex-1">
									<Input
										value={row.identifier}
										onChange={(e) =>
											updateRow(index, { identifier: e.target.value })
										}
										placeholder={probingPlaceholder}
										spellCheck={false}
										className="pr-16 font-mono text-xs placeholder:font-sans"
										disabled={busy}
									/>
									{identifierKind(row.identifier) ? (
										<span
											className="pointer-events-none absolute inset-y-0 right-2 flex items-center text-muted-foreground/60 text-xs"
											aria-hidden
										>
											{identifierKind(row.identifier)}
										</span>
									) : null}
								</div>
							</div>
						</li>
					))}
				</ul>

				<DialogFooter className="shrink-0 gap-2">
					<Button
						type="button"
						variant="outline"
						onClick={() => onOpenChange(false)}
						disabled={busy}
					>
						{t("importLocalPdf.cancel")}
					</Button>
					<Button type="button" onClick={handleConfirm} disabled={!canSubmit}>
						{probing
							? t("importLocalPdf.recognizing")
							: t("importLocalPdf.confirm")}
					</Button>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
