/**
 * Prompts for the Agent translation provider.
 * Generic surface + PDF selection variant.
 */

/** True when `text` is a numbered batch payload (`[[1]] …`, `[[2]] …`). */
function hasNumberedMarkers(text: string): boolean {
	return /\[{2}\s*\d+\s*\]{2}/.test(text);
}

export function buildTranslatePrompt(opts: {
	text: string;
	targetLangName: string;
	page?: number;
	surface?: string;
}): string {
	const text = opts.text.trim();
	const parts = [
		"You are a translation assistant in Agentero.",
		`Translate the text below into ${opts.targetLangName}. Preserve technical terms and formulas.`,
		"Return only the translation, without commentary.",
	];
	if (opts.surface === "pdf-selection" && opts.page != null) {
		parts.push(`Source: research paper PDF, page ${opts.page}.`);
	}
	if (hasNumberedMarkers(text)) {
		parts.push(
			"The text contains several paragraphs, each prefixed with a [[n]] marker. " +
				"Translate every paragraph and keep the same [[n]] markers and the same " +
				"number of paragraphs in the output.",
		);
	}
	parts.push("Text:", `> ${text}`);
	return parts.join("\n\n");
}
