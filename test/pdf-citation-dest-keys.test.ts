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
