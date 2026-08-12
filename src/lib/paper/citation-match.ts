/**
 * Match text extracted from a PDF bibliography region against the parsed
 * citation entries of the `agentero-cite.json` sidecar. The geometric
 * extraction (see `mergeBibliographyEntryAtY`) is noisy — the hover preview
 * shows the clean sidecar text of the best match instead.
 */
import type { Citation } from "@/lib/paper/refs";

const MATCH_THRESHOLD = 0.5;
/** Containment is permissive on short fragments; require a real snippet. */
const MIN_MATCH_CHARS = 24;

/** Lowercase, strip diacritics/punctuation, collapse whitespace. */
function normalizeForMatch(text: string): string {
	return text
		.toLowerCase()
		.normalize("NFKD")
		.replace(/[\u0300-\u036f]/g, "")
		.replace(/[^\p{L}\p{N}\s]/gu, " ")
		.replace(/\s+/g, " ")
		.trim();
}

function bigramCounts(text: string): Map<string, number> {
	const counts = new Map<string, number>();
	for (let i = 0; i < text.length - 1; i++) {
		const gram = text.slice(i, i + 2);
		counts.set(gram, (counts.get(gram) ?? 0) + 1);
	}
	return counts;
}

/**
 * Raw containment of `extracted` in `candidate` over character-bigram counts:
 * the share of the extracted text's bigrams that appear in the candidate.
 * Tolerates partial extraction; drops when extra (wrong) lines are included.
 */
export function citationTextMatchScore(
	extracted: string,
	candidate: string,
): number {
	const a = bigramCounts(normalizeForMatch(extracted));
	const b = bigramCounts(normalizeForMatch(candidate));
	if (!a.size || !b.size) return 0;
	let intersection = 0;
	let total = 0;
	for (const [gram, count] of a) {
		total += count;
		intersection += Math.min(count, b.get(gram) ?? 0);
	}
	return total ? intersection / total : 0;
}

/**
 * Best sidecar entry for extracted bibliography text, or null when nothing
 * matches well enough. Bigrams are weighted by inverse document frequency
 * across the citation list, so boilerplate shared by every entry ("in",
 * "20", ...) cannot tip short generic fragments (section headings) into a
 * false match, while entry-specific text still scores high.
 */
export function matchCitationByText(
	text: string,
	citations: Citation[],
): Citation | null {
	const extracted = normalizeForMatch(text);
	if (extracted.length < MIN_MATCH_CHARS || !citations.length) return null;

	const candidates = citations.map((citation) => ({
		citation,
		grams: bigramCounts(
			normalizeForMatch(citation.raw ?? citation.metadata.title ?? ""),
		),
	}));
	const docFreq = new Map<string, number>();
	for (const { grams } of candidates) {
		for (const gram of grams.keys()) {
			docFreq.set(gram, (docFreq.get(gram) ?? 0) + 1);
		}
	}
	const n = candidates.length;
	const weight = (gram: string) => Math.log(1 + n / (docFreq.get(gram) ?? 1));

	const extractedGrams = bigramCounts(extracted);
	let best: Citation | null = null;
	let bestScore = MATCH_THRESHOLD;
	for (const { citation, grams } of candidates) {
		if (!grams.size) continue;
		let intersection = 0;
		let total = 0;
		for (const [gram, count] of extractedGrams) {
			const w = weight(gram);
			total += count * w;
			intersection += Math.min(count, grams.get(gram) ?? 0) * w;
		}
		const score = total ? intersection / total : 0;
		if (score > bestScore) {
			bestScore = score;
			best = citation;
		}
	}
	return best;
}
