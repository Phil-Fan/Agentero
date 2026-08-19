import { SearchCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
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
import { useOverlayRegistration } from "@/hooks/use-overlay-registration";
import { basenameOf } from "@/lib/core/path";
import {
	type PdfIdentProbe,
	probePdfIdents,
	resolveIdentifierMetadata,
} from "@/lib/paper/api";
import { slugFromPdfPath, titleFromPdfPath } from "@/lib/paper/local-pdf-meta";
import type {
	LocalPdfExtraMeta,
	LocalPdfImportEntry,
} from "@/lib/paper/lookup";

type DraftRow = {
	filePath: string;
	/** Original filename for UI + default title/id (not the staging path). */
	sourceName: string;
	title: string;
	authors: string;
	year: string;
	id: string;
	doi: string;
	arxivId: string;
	/** Structured fields from probe/Fetch, submitted as `extra`. */
	extra?: LocalPdfExtraMeta;
};

export type ImportLocalPdfDraftItem = {
	path: string;
	sourceName: string;
};

function draftsFromItems(items: ImportLocalPdfDraftItem[]): DraftRow[] {
	return items.map((item) => {
		const nameForMeta = item.sourceName || item.path;
		return {
			filePath: item.path,
			sourceName: item.sourceName || basenameOf(item.path),
			title: titleFromPdfPath(nameForMeta),
			authors: "",
			year: "",
			id: slugFromPdfPath(nameForMeta),
			doi: "",
			arxivId: "",
		};
	});
}

function applyProbe(row: DraftRow, probe: PdfIdentProbe): DraftRow {
	if (probe.status === "no-match" || probe.status === "error") return row;
	const next: DraftRow = { ...row };
	if (probe.title?.trim()) next.title = probe.title.trim();
	if (probe.authors.length) next.authors = probe.authors.join(", ");
	if (probe.year != null) next.year = String(probe.year);
	if (probe.doi?.trim()) next.doi = probe.doi.trim();
	if (probe.arxivId?.trim()) next.arxivId = probe.arxivId.trim();
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
	const [probing, setProbing] = useState(false);
	const [probeStatus, setProbeStatus] = useState<Record<string, string>>({});
	const [fetching, setFetching] = useState<Record<string, boolean>>({});

	useOverlayRegistration("import-local-pdf", open, () => onOpenChange(false));

	useEffect(() => {
		if (!open) return;
		setRows(draftsFromItems(items));
		setDest(parentDir || "papers");
		setProbeStatus({});
		setFetching({});
		// Best-effort recognition: failures leave filename-derived defaults.
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
				/* keep filename defaults */
			})
			.finally(() => setProbing(false));
	}, [open, items, parentDir]);

	const canSubmit = useMemo(() => {
		if (!rows.length || busy || probing) return false;
		return rows.every(
			(r) => r.title.trim().length > 0 && r.id.trim().length > 0,
		);
	}, [rows, busy, probing]);

	const updateRow = (index: number, patch: Partial<DraftRow>) => {
		setRows((prev) =>
			prev.map((r, i) => (i === index ? { ...r, ...patch } : r)),
		);
	};

	const handleFetch = async (index: number, row: DraftRow) => {
		const text = row.doi.trim() || row.arxivId.trim();
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
					if (meta.doi?.trim()) next.doi = meta.doi.trim();
					if (meta.arxivId?.trim()) next.arxivId = meta.arxivId.trim();
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
			const hasIdentifiers = Boolean(r.doi.trim() || r.arxivId.trim());
			return {
				filePath: r.filePath,
				title: r.title.trim(),
				authors: authors.length ? authors : undefined,
				year: Number.isFinite(yearNum) ? yearNum : undefined,
				id: r.id.trim() || undefined,
				doi: r.doi.trim() || undefined,
				arxivId: r.arxivId.trim() || undefined,
				extra: hasIdentifiers ? r.extra : undefined,
			};
		});
		onConfirm(entries, dest.trim() || "papers");
	};

	const probeBadge = (row: DraftRow) => {
		if (probing) {
			return (
				<span className="text-muted-foreground text-xs">
					{t("importLocalPdf.recognizing")}
				</span>
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
		return null;
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

				<div className="shrink-0 space-y-1.5">
					<Label htmlFor="import-pdf-parent" className="text-xs">
						{t("importLocalPdf.parentDir")}
					</Label>
					<Input
						id="import-pdf-parent"
						value={dest}
						onChange={(e) => setDest(e.target.value)}
						disabled={busy}
						spellCheck={false}
						className="font-mono text-xs"
					/>
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
								{probeBadge(row)}
							</div>
							<div className="grid grid-cols-[1fr_5.5rem] gap-2">
								<div className="space-y-1">
									<Label className="text-xs">
										{t("importLocalPdf.fieldTitle")}
									</Label>
									<Input
										value={row.title}
										onChange={(e) =>
											updateRow(index, { title: e.target.value })
										}
										disabled={busy}
									/>
								</div>
								<div className="space-y-1">
									<Label className="text-xs">
										{t("importLocalPdf.fieldYear")}
									</Label>
									<Input
										value={row.year}
										onChange={(e) => updateRow(index, { year: e.target.value })}
										inputMode="numeric"
										placeholder="2024"
										disabled={busy}
									/>
								</div>
							</div>
							<div className="grid grid-cols-[1fr_1fr] gap-2">
								<div className="space-y-1">
									<Label className="text-xs">
										{t("importLocalPdf.fieldAuthors")}
									</Label>
									<Input
										value={row.authors}
										onChange={(e) =>
											updateRow(index, { authors: e.target.value })
										}
										placeholder={t("importLocalPdf.authorsPlaceholder")}
										disabled={busy}
									/>
								</div>
								<div className="space-y-1">
									<Label className="text-xs">
										{t("importLocalPdf.fieldId")}
									</Label>
									<Input
										value={row.id}
										onChange={(e) => updateRow(index, { id: e.target.value })}
										spellCheck={false}
										className="font-mono text-xs"
										disabled={busy}
									/>
								</div>
							</div>
							<div className="grid grid-cols-[1fr_1fr_auto] items-end gap-2">
								<div className="space-y-1">
									<Label className="text-xs">
										{t("importLocalPdf.fieldDoi")}
									</Label>
									<Input
										value={row.doi}
										onChange={(e) => updateRow(index, { doi: e.target.value })}
										placeholder="10.1000/xyz123"
										spellCheck={false}
										className="font-mono text-xs"
										disabled={busy}
									/>
								</div>
								<div className="space-y-1">
									<Label className="text-xs">
										{t("importLocalPdf.fieldArxivId")}
									</Label>
									<Input
										value={row.arxivId}
										onChange={(e) =>
											updateRow(index, { arxivId: e.target.value })
										}
										placeholder="1706.03762"
										spellCheck={false}
										className="font-mono text-xs"
										disabled={busy}
									/>
								</div>
								<Button
									type="button"
									variant="outline"
									size="sm"
									className="h-8"
									disabled={
										busy ||
										fetching[row.filePath] ||
										!(row.doi.trim() || row.arxivId.trim())
									}
									onClick={() => void handleFetch(index, row)}
								>
									{fetching[row.filePath]
										? t("importLocalPdf.fetching")
										: t("importLocalPdf.fetch")}
								</Button>
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
