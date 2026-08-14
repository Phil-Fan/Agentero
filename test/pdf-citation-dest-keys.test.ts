import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
	buildCitationDestKeyMap,
	citationDestKey,
} from "@/lib/pdf/citation-dest-keys";

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
