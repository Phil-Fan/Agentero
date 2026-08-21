/**
 * Exact in-text citation → bibliography entry resolution via hyperref named
 * destinations.
 *
 * LaTeX/hyperref writes each in-text citation link as a GoTo action to a named
 * destination `cite.<bibtexKey>`, and our reference sidecar carries the same key
 * as `Citation.rawKey`. PDFium resolves the name away (the viewer only sees
 * `pageIndex` + y), so we read the name tree straight from the PDF bytes once
 * and index destinations by their coordinates — the same coordinates the viewer
 * gets back from a link target.
 *
 * Measured over 31 real papers / 3703 in-text citation links: 95% resolved, 0
 * wrong. Coordinates shared by several keys are dropped rather than guessed, so
 * callers fall back to text matching instead of showing a wrong reference.
 */

import {
	PDFArray,
	type PDFContext,
	PDFDict,
	PDFDocument,
	PDFHexString,
	PDFName,
	PDFNumber,
	PDFString,
} from "pdf-lib";

const CITE_DEST_PREFIX = "cite.";

/** Coordinate key: destination page + PDF-native y (as PDFium reports it). */
export function citationDestKey(pageIndex: number, pdfY: number): string {
	return `${pageIndex}:${pdfY.toFixed(1)}`;
}

/** `pageIndex:pdfY` → BibTeX key of the bibliography entry at that destination. */
export type CitationDestKeyMap = ReadonlyMap<string, string>;

/** Kind of numbered object a hyperref cross-reference points at. */
export type CrossrefKind = "figure" | "table" | "equation" | "algorithm";

/** `pageIndex:pdfY` → kind of the numbered object at that destination. */
export type CrossrefDestMap = ReadonlyMap<string, CrossrefKind>;

/** Both destination indexes parsed from one PDF in a single object-tree walk. */
export type PdfDestMaps = {
	/** In-text citation destinations (`cite.<key>`). */
	cites: CitationDestKeyMap;
	/** Figure / table / equation / algorithm destinations (`\ref` targets). */
	crossrefs: CrossrefDestMap;
};

/**
 * hyperref destination name → cross-reference kind, or null when the name is
 * not a numbered figure/table/equation/algorithm anchor. LaTeX packages write
 * `figure.<n>`, `table.<n>`, `equation.<n>` / `subequation.<n>`, and
 * `algorithm.<n>` (algorithm2e uses `algocf.<n>`).
 */
function crossrefKindFromName(name: string): CrossrefKind | null {
	if (name.startsWith("figure.")) return "figure";
	if (name.startsWith("subfigure.")) return "figure";
	if (name.startsWith("table.")) return "table";
	if (name.startsWith("equation.")) return "equation";
	if (name.startsWith("subequation.")) return "equation";
	if (name.startsWith("algorithm.")) return "algorithm";
	if (name.startsWith("algocf.")) return "algorithm";
	return null;
}

/** Walk a /Dests name tree into `name → destination array`. */
function collectNameTree(
	context: PDFContext,
	node: PDFDict | undefined,
	out: Map<string, PDFArray>,
): void {
	if (!node) return;

	const names = context.lookup(node.get(PDFName.of("Names")));
	if (names instanceof PDFArray) {
		const arr = names.asArray();
		for (let i = 0; i + 1 < arr.length; i += 2) {
			const rawName = context.lookup(arr[i]);
			if (!(rawName instanceof PDFString || rawName instanceof PDFHexString)) {
				continue;
			}
			// A named destination is the array itself or a dict wrapping it in /D.
			const value = context.lookup(arr[i + 1]);
			let dest: PDFArray | undefined;
			if (value instanceof PDFArray) dest = value;
			else if (value instanceof PDFDict) {
				const inner = context.lookup(value.get(PDFName.of("D")));
				if (inner instanceof PDFArray) dest = inner;
			}
			if (dest) out.set(rawName.asString(), dest);
		}
	}

	const kids = context.lookup(node.get(PDFName.of("Kids")));
	if (kids instanceof PDFArray) {
		for (const kid of kids.asArray()) {
			const child = context.lookup(kid);
			if (child instanceof PDFDict) collectNameTree(context, child, out);
		}
	}
}

/**
 * Resolve a named-destination array to its coordinate key, or null when it
 * carries no usable `/XYZ` y (only XYZ destinations pin a vertical position).
 */
function destCoordKey(
	dest: PDFArray,
	context: PDFContext,
	pageRefs: string[],
): string | null {
	const arr = dest.asArray();
	// [pageRef /XYZ x y zoom] — only XYZ carries a usable y.
	if (arr.length < 4 || arr[1]?.toString() !== "/XYZ") return null;
	const pageIndex = pageRefs.indexOf(arr[0].toString());
	if (pageIndex < 0) return null;
	const y = context.lookup(arr[3]);
	if (!(y instanceof PDFNumber)) return null;
	return citationDestKey(pageIndex, y.asNumber());
}

/**
 * Collapse a coordinate → value index, dropping coordinates shared by two
 * different values (ambiguous, never guessed — callers fall back instead).
 */
function resolveUnambiguous<T>(byCoord: Map<string, T | null>): Map<string, T> {
	const out = new Map<string, T>();
	for (const [coord, value] of byCoord) {
		if (value != null) out.set(coord, value);
	}
	return out;
}

/**
 * Build both destination indexes for one PDF in a single object-tree walk.
 * Returns empty maps for PDFs without hyperref named destinations.
 */
export async function buildPdfDestMaps(
	bytes: ArrayBuffer | Uint8Array,
): Promise<PdfDestMaps> {
	const empty: PdfDestMaps = { cites: new Map(), crossrefs: new Map() };
	const doc = await PDFDocument.load(bytes, { updateMetadata: false });
	const context = doc.context;

	const names = context.lookup(doc.catalog.get(PDFName.of("Names")));
	if (!(names instanceof PDFDict)) return empty;
	const dests = context.lookup(names.get(PDFName.of("Dests")));
	if (!(dests instanceof PDFDict)) return empty;

	const namedDests = new Map<string, PDFArray>();
	collectNameTree(context, dests, namedDests);
	if (!namedDests.size) return empty;

	const pageRefs = doc.getPages().map((page) => page.ref.toString());
	const citeByCoord = new Map<string, string | null>();
	const crossByCoord = new Map<string, CrossrefKind | null>();

	const stash = <T>(map: Map<string, T | null>, coord: string, value: T) => {
		const existing = map.get(coord);
		if (existing === undefined) map.set(coord, value);
		// Two entries anchored at the same spot: ambiguous, never guess.
		else if (existing !== value) map.set(coord, null);
	};

	for (const [name, dest] of namedDests) {
		if (name.startsWith(CITE_DEST_PREFIX)) {
			const coord = destCoordKey(dest, context, pageRefs);
			if (coord) stash(citeByCoord, coord, name.slice(CITE_DEST_PREFIX.length));
			continue;
		}
		const kind = crossrefKindFromName(name);
		if (kind) {
			const coord = destCoordKey(dest, context, pageRefs);
			if (coord) stash(crossByCoord, coord, kind);
		}
	}

	return {
		cites: resolveUnambiguous(citeByCoord),
		crossrefs: resolveUnambiguous(crossByCoord),
	};
}

/**
 * Build the coordinate → BibTeX key index for one PDF.
 * Returns an empty map for PDFs without hyperref citation destinations.
 */
export async function buildCitationDestKeyMap(
	bytes: ArrayBuffer | Uint8Array,
): Promise<CitationDestKeyMap> {
	return (await buildPdfDestMaps(bytes)).cites;
}
