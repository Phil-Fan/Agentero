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
 * Build the coordinate → BibTeX key index for one PDF.
 * Returns an empty map for PDFs without hyperref citation destinations.
 */
export async function buildCitationDestKeyMap(
	bytes: ArrayBuffer | Uint8Array,
): Promise<CitationDestKeyMap> {
	const doc = await PDFDocument.load(bytes, { updateMetadata: false });
	const context = doc.context;

	const names = context.lookup(doc.catalog.get(PDFName.of("Names")));
	if (!(names instanceof PDFDict)) return new Map();
	const dests = context.lookup(names.get(PDFName.of("Dests")));
	if (!(dests instanceof PDFDict)) return new Map();

	const namedDests = new Map<string, PDFArray>();
	collectNameTree(context, dests, namedDests);
	if (!namedDests.size) return new Map();

	const pageRefs = doc.getPages().map((page) => page.ref.toString());
	const byCoord = new Map<string, string | null>();

	for (const [name, dest] of namedDests) {
		if (!name.startsWith(CITE_DEST_PREFIX)) continue;
		const arr = dest.asArray();
		// [pageRef /XYZ x y zoom] — only XYZ carries a usable y.
		if (arr.length < 4 || arr[1]?.toString() !== "/XYZ") continue;
		const pageIndex = pageRefs.indexOf(arr[0].toString());
		if (pageIndex < 0) continue;
		const y = context.lookup(arr[3]);
		if (!(y instanceof PDFNumber)) continue;

		const coord = citationDestKey(pageIndex, y.asNumber());
		const key = name.slice(CITE_DEST_PREFIX.length);
		const existing = byCoord.get(coord);
		if (existing === undefined) byCoord.set(coord, key);
		// Two entries anchored at the same spot: ambiguous, never guess.
		else if (existing !== key) byCoord.set(coord, null);
	}

	const out = new Map<string, string>();
	for (const [coord, key] of byCoord) {
		if (key) out.set(coord, key);
	}
	return out;
}
