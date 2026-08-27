import { readFileSync } from "node:fs";
import type { PDFArray, PDFContext } from "pdf-lib";
import { describe, expect, it } from "vitest";
import {
	acsCrossrefParser,
	buildCitationDestKeyMap,
	buildPdfDestMaps,
	citationDestKey,
	fitHCoordResolver,
	fitRCoordResolver,
	hyperrefCrossrefParser,
	xyzCoordResolver,
} from "@/lib/pdf/citation-dest-keys";
import { extractCrossrefLabel } from "@/lib/pdf/crossref-resolve";

/**
 * Real-PDF fixtures live outside the repo, so these cases are opt-in: point
 * AGENTERO_TEST_PAPER at a paper folder to exercise the parser end to end.
 */
const paperDir = process.env.AGENTERO_TEST_PAPER;

describe("citation destination keys", () => {
	it("returns an empty map for a PDF without hyperref cite destinations", async () => {
		// Minimal PDF with no /Names tree at all.
		const { PDFDocument } = await import("pdf-lib");
		const doc = await PDFDocument.create();
		doc.addPage();
		const map = await buildCitationDestKeyMap(await doc.save());
		expect(map.size).toBe(0);
	});

	it("keys destinations by page and PDF-native y", () => {
		expect(citationDestKey(9, 566.93)).toBe("9:566.9");
		expect(citationDestKey(0, 0)).toBe("0:0.0");
	});

	it("resolves a generated many-page hyperref PDF; parse cost is worker-bound", async () => {
		const { PDFDocument, PDFName, PDFNumber, PDFString } = await import(
			"pdf-lib"
		);
		const pageCount = 1000;
		const citeCount = 10000;
		const doc = await PDFDocument.create();
		const pages = Array.from({ length: pageCount }, () => {
			const page = doc.addPage();
			// Some real content per page so the object tree is not degenerate.
			for (let r = 0; r < 8; r++) {
				page.drawRectangle({ x: 10 + r * 12, y: 10, width: 10, height: 10 });
			}
			return page;
		});
		const context = doc.context;
		// hyperref layout: /Names → /Dests → Names [ (cite.<key>) [page /XYZ x y z] … ]
		const namePairs = [];
		for (let i = 0; i < citeCount; i++) {
			const page = pages[i % pageCount];
			namePairs.push(PDFString.of(`cite.key${i}`));
			namePairs.push(
				context.obj([
					page.ref,
					PDFName.of("XYZ"),
					PDFNumber.of(0),
					PDFNumber.of(720 - Math.floor(i / pageCount) * 12),
					PDFNumber.of(0),
				]),
			);
		}
		const dests = context.obj({ Names: context.obj(namePairs) });
		doc.catalog.set(PDFName.of("Names"), context.obj({ Dests: dests }));
		const bytes = await doc.save();

		const started = performance.now();
		const map = await buildCitationDestKeyMap(bytes);
		const ms = performance.now() - started;

		expect(map.size).toBe(citeCount);
		expect(map.get(citationDestKey(0, 720))).toBe("key0");
		expect(map.get(citationDestKey(999, 720))).toBe("key999");
		expect(map.get(citationDestKey(0, 708))).toBe("key1000");
		// pdf-lib has no lazy parsing: PDFDocument.load walks the whole object
		// tree, so cost scales with the PDF (seconds on real 100MB+ papers).
		// That is why production builds this map inside a worker — record the
		// synchronous cost here as the perf evidence.
		console.info(
			`buildCitationDestKeyMap: ${pageCount} pages / ${citeCount} dests parsed synchronously in ${Math.round(ms)}ms`,
		);
		expect(ms).toBeGreaterThan(0);
	});

	it.skipIf(!paperDir)(
		"resolves hyperref cite keys to sidecar rawKeys",
		async () => {
			const dir = paperDir as string;
			const name = dir.replace(/\/$/, "").split("/").pop();
			const map = await buildCitationDestKeyMap(
				readFileSync(`${dir}/${name}.pdf`),
			);
			const sidecar = JSON.parse(
				readFileSync(`${dir}/source/agentero-cite.json`, "utf8"),
			);
			const rawKeys = new Set(
				sidecar.citations
					.map((c: { rawKey?: string }) => c.rawKey)
					.filter(Boolean),
			);
			expect(map.size).toBeGreaterThan(0);
			const resolved = [...map.values()].filter((key) => rawKeys.has(key));
			// Every mapped destination should name a citation we parsed.
			expect(resolved.length).toBe(map.size);
		},
	);
});

describe("cross-reference name parsers", () => {
	it("parses standard hyperref names", () => {
		expect(hyperrefCrossrefParser("figure.1")).toEqual({ kind: "figure" });
		expect(hyperrefCrossrefParser("subfigure.1a")).toEqual({ kind: "figure" });
		expect(hyperrefCrossrefParser("figure.caption.2")).toEqual({
			kind: "figure",
		});
		expect(hyperrefCrossrefParser("table.2")).toEqual({ kind: "table" });
		expect(hyperrefCrossrefParser("table.caption.2")).toEqual({
			kind: "table",
		});
		expect(hyperrefCrossrefParser("equation.3")).toEqual({ kind: "equation" });
		expect(hyperrefCrossrefParser("subequation.3a")).toEqual({
			kind: "equation",
		});
		expect(hyperrefCrossrefParser("equation.E.4")).toEqual({
			kind: "equation",
		});
		expect(hyperrefCrossrefParser("algorithm.1")).toEqual({
			kind: "algorithm",
		});
		expect(hyperrefCrossrefParser("algocf.1")).toEqual({ kind: "algorithm" });
		expect(hyperrefCrossrefParser("algocf.caption.1")).toEqual({
			kind: "algorithm",
		});
		expect(hyperrefCrossrefParser("section.1")).toBeNull();
		expect(hyperrefCrossrefParser("algocfline.1")).toBeNull();
	});

	it("parses ACS mk:* names", () => {
		expect(acsCrossrefParser("mk:fig1")).toEqual({ kind: "figure" });
		expect(acsCrossrefParser("mk:figS1")).toEqual({ kind: "figure" });
		expect(acsCrossrefParser("mk:tbl1")).toEqual({ kind: "table" });
		expect(acsCrossrefParser("mk:eq1")).toEqual({ kind: "equation" });
		expect(acsCrossrefParser("mk:ref1")).toBeNull();
		expect(acsCrossrefParser("mk:ath1")).toBeNull();
	});
});

describe("destination coordinate resolvers", () => {
	async function buildTestDest(
		mode: string,
		params: number[],
	): Promise<{
		dest: PDFArray;
		context: PDFContext;
		pageRefs: string[];
	}> {
		const { PDFDocument, PDFName, PDFNumber } = await import("pdf-lib");
		const doc = await PDFDocument.create();
		const page = doc.addPage();
		const context = doc.context;
		const arr = [page.ref, PDFName.of(mode), ...params.map(PDFNumber.of)];
		return { dest: context.obj(arr), context, pageRefs: [page.ref.toString()] };
	}

	it("resolves /XYZ destinations", async () => {
		const { dest, context, pageRefs } = await buildTestDest("XYZ", [0, 500, 0]);
		const result = xyzCoordResolver(dest, context, pageRefs);
		expect(result).toEqual({ pageIndex: 0, pdfY: 500 });
	});

	it("resolves /FitR destinations using the top edge", async () => {
		const { dest, context, pageRefs } = await buildTestDest(
			"FitR",
			[0, 100, 200, 500],
		);
		const result = fitRCoordResolver(dest, context, pageRefs);
		expect(result).toEqual({ pageIndex: 0, pdfY: 500 });
	});

	it("resolves /FitH destinations", async () => {
		const { dest, context, pageRefs } = await buildTestDest("FitH", [600]);
		const result = fitHCoordResolver(dest, context, pageRefs);
		expect(result).toEqual({ pageIndex: 0, pdfY: 600 });
	});
});

describe("buildPdfDestMaps with mixed conventions", () => {
	async function buildMixedPdf() {
		const { PDFDocument, PDFName, PDFNumber, PDFString } = await import(
			"pdf-lib"
		);
		const doc = await PDFDocument.create();
		const page = doc.addPage();
		const context = doc.context;

		function dest(mode: string, params: number[]) {
			return context.obj([
				page.ref,
				PDFName.of(mode),
				...params.map(PDFNumber.of),
			]);
		}

		const namePairs = [
			// Standard hyperref
			PDFString.of("figure.1"),
			dest("XYZ", [0, 400, 0]),
			PDFString.of("table.1"),
			dest("XYZ", [0, 300, 0]),
			PDFString.of("cite.key2024"),
			dest("XYZ", [0, 120, 0]),
			// ACS style — fig and tbl share the same /FitR top to exercise conflict
			// resolution via crossrefKinds.
			PDFString.of("mk:fig1"),
			dest("FitR", [0, 350, 500, 400]),
			PDFString.of("mk:tbl1"),
			dest("FitR", [0, 350, 500, 400]),
			PDFString.of("mk:ref1"),
			dest("FitR", [0, 50, 500, 100]),
		];

		const dests = context.obj({ Names: context.obj(namePairs) });
		doc.catalog.set(PDFName.of("Names"), context.obj({ Dests: dests }));
		return { bytes: await doc.save(), page };
	}

	it("resolves both hyperref and ACS crossrefs in one PDF", async () => {
		const { bytes } = await buildMixedPdf();
		const maps = await buildPdfDestMaps(bytes);

		// Standard hyperref targets are unambiguous.
		expect(maps.crossrefs.get(citationDestKey(0, 300))).toBe("table");

		// ACS mk:fig1, mk:tbl1 and the hyperref figure.1 all share y=400, so the
		// unambiguous map drops it, but both kinds survive in crossrefKinds.
		const conflictCoord = citationDestKey(0, 400);
		expect(maps.crossrefs.get(conflictCoord)).toBeUndefined();
		expect(maps.crossrefKinds.get(conflictCoord)?.sort()).toEqual([
			"figure",
			"table",
		]);

		expect(maps.cites.get(citationDestKey(0, 120))).toBe("key2024");
		expect(maps.cites.get(citationDestKey(0, 100))).toBe("mk:ref1");
	});

	it("allows custom parsers to be injected", async () => {
		const { bytes } = await buildMixedPdf();
		const customParser = (name: string) =>
			name.startsWith("custom:") ? { kind: "figure" as const } : null;

		const maps = await buildPdfDestMaps(bytes, {
			crossrefParsers: [customParser],
			citationParsers: [],
			coordResolvers: [xyzCoordResolver],
		});

		// Only the custom parser runs; standard names are ignored.
		expect(maps.crossrefs.get(citationDestKey(0, 400))).toBeUndefined();
		expect(maps.cites.size).toBe(0);
	});

	it("keeps conflicting kinds in crossrefKinds for fallback disambiguation", async () => {
		const { bytes } = await buildMixedPdf();
		const maps = await buildPdfDestMaps(bytes);

		// mk:fig1, mk:tbl1 and hyperref figure.1 share the same coordinate on page 1.
		const conflictCoord = citationDestKey(0, 400);
		expect(maps.crossrefs.get(conflictCoord)).toBeUndefined();
		expect(maps.crossrefKinds.get(conflictCoord)?.sort()).toEqual([
			"figure",
			"table",
		]);

		// Unambiguous coordinates still appear in both maps.
		expect(maps.crossrefs.get(citationDestKey(0, 300))).toBe("table");
		expect(maps.crossrefKinds.get(citationDestKey(0, 300))).toEqual(["table"]);
	});
});

describe("crossref label extraction", () => {
	it("extracts figure labels", () => {
		expect(extractCrossrefLabel("see Figure 1 for details")).toEqual({
			kind: "figure",
			number: 1,
		});
		expect(extractCrossrefLabel("Fig. 2 shows")).toEqual({
			kind: "figure",
			number: 2,
		});
	});

	it("extracts table labels", () => {
		expect(extractCrossrefLabel("in Table 3")).toEqual({
			kind: "table",
			number: 3,
		});
	});

	it("extracts equation labels", () => {
		expect(extractCrossrefLabel("using Eq. (4)")).toEqual({
			kind: "equation",
			number: 4,
		});
		expect(extractCrossrefLabel("Equation 5")).toEqual({
			kind: "equation",
			number: 5,
		});
	});

	it("returns null for non-crossref text", () => {
		expect(extractCrossrefLabel("see Section 1")).toBeNull();
		expect(extractCrossrefLabel("hello world")).toBeNull();
	});
});
