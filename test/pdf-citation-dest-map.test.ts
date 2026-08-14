import { beforeEach, describe, expect, it, vi } from "vitest";
import type { CitationDestKeyMap } from "@/lib/pdf/citation-dest-keys";
import {
	clearCitationDestKeyMapCache,
	getCitationDestKeyMapCached,
} from "@/lib/pdf/citation-dest-map";

describe("citation dest key map cache", () => {
	beforeEach(() => {
		clearCitationDestKeyMapCache();
	});

	it("parses once and serves the second request from cache", async () => {
		const map: CitationDestKeyMap = new Map([["3:100.0", "smith2020"]]);
		const parse = vi.fn().mockResolvedValue(map);

		const first = await getCitationDestKeyMapCached(
			"/vault/papers/p1/p1.pdf:1234",
			"viewer-bytes",
			parse,
		);
		const second = await getCitationDestKeyMapCached(
			"/vault/papers/p1/p1.pdf:1234",
			"viewer-bytes",
			parse,
		);

		expect(parse).toHaveBeenCalledTimes(1);
		expect(second).toBe(first);
		expect(second.get("3:100.0")).toBe("smith2020");
	});

	it("dedupes concurrent requests for the same PDF", async () => {
		const parse = vi.fn().mockResolvedValue(new Map());
		const [a, b] = await Promise.all([
			getCitationDestKeyMapCached("k:1", "disk", parse),
			getCitationDestKeyMapCached("k:1", "disk", parse),
		]);
		expect(parse).toHaveBeenCalledTimes(1);
		expect(b).toBe(a);
	});

	it("keys by pdf path + size, so a different PDF parses again", async () => {
		const parse = vi.fn().mockResolvedValue(new Map());
		await getCitationDestKeyMapCached("/a.pdf:10", "disk", parse);
		await getCitationDestKeyMapCached("/a.pdf:20", "disk", parse);
		await getCitationDestKeyMapCached("/b.pdf:10", "disk", parse);
		expect(parse).toHaveBeenCalledTimes(3);
	});

	it("does not cache failures, allowing a retry", async () => {
		const parse = vi
			.fn()
			.mockRejectedValueOnce(new Error("boom"))
			.mockResolvedValueOnce(new Map([["0:0.0", "ok"]]));

		await expect(
			getCitationDestKeyMapCached("k:2", "disk", parse),
		).rejects.toThrow("boom");
		const retried = await getCitationDestKeyMapCached("k:2", "disk", parse);
		expect(parse).toHaveBeenCalledTimes(2);
		expect(retried.get("0:0.0")).toBe("ok");
	});
});
