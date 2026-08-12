import { describe, expect, it } from "vitest";
import {
	citationTextMatchScore,
	matchCitationByText,
} from "@/lib/paper/citation-match";
import type { Citation } from "@/lib/paper/refs";

function citation(id: string, raw: string): Citation {
	return {
		id,
		raw,
		metadata: {},
		source: "bbl",
		status: "resolved",
	};
}

const YAO = citation(
	"cite-yao",
	"Shunyu Yao, Jeffrey Zhao, Dian Yu, Nan Du, Izhak Shafran, Karthik Narasimhan, and Yuan Cao. React: Synergizing reasoning and acting in language models. In ICLR, 2023.",
);
const VASWANI = citation(
	"cite-vaswani",
	"Ashish Vaswani, Noam Shazeer, Niki Parmar, Jakob Uszkoreit, Llion Jones, Aidan N Gomez, Lukasz Kaiser, and Illia Polosukhin. Attention is all you need. In NeurIPS, 2017.",
);
const BROWN = citation(
	"cite-brown",
	"Tom B Brown, Benjamin Mann, Nick Ryder, Melanie Subbiah, Jared Kaplan, Prafulla Dhariwal, Arvind Neelakantan, et al. Language models are few-shot learners. In NeurIPS, 2020.",
);

describe("citation text matching", () => {
	it("scores near-identical text high and unrelated text low", () => {
		expect(citationTextMatchScore(YAO.raw as string, YAO.raw as string)).toBe(
			1,
		);
		expect(
			citationTextMatchScore(YAO.raw as string, VASWANI.raw as string),
		).toBeLessThan(0.4);
	});

	it("matches noisy PDF extraction (extra whitespace, line breaks)", () => {
		const noisy =
			"[3] Shunyu Yao, Jeffrey Zhao, Dian Yu, Nan Du, Izhak\n  Shafran, Karthik Narasimhan, and Yuan Cao. React: Synergizing\n  reasoning and acting in language models. In ICLR,";
		const match = matchCitationByText(noisy, [VASWANI, YAO, BROWN]);
		expect(match?.id).toBe("cite-yao");
	});

	it("matches partial extraction of an entry", () => {
		const partial = "Attention is all you need. In NeurIPS, 2017.";
		const match = matchCitationByText(partial, [YAO, VASWANI, BROWN]);
		expect(match?.id).toBe("cite-vaswani");
	});

	it("returns null for unrelated garbage", () => {
		expect(
			matchCitationByText("1 Introduction and related work", [
				YAO,
				VASWANI,
				BROWN,
			]),
		).toBeNull();
	});

	it("falls back to metadata title when raw is missing", () => {
		const titled: Citation = {
			id: "cite-titled",
			metadata: { title: "Attention is all you need" },
			source: "s2",
			status: "resolved",
		};
		const match = matchCitationByText("Attention is all you need.", [titled]);
		expect(match?.id).toBe("cite-titled");
	});
});
