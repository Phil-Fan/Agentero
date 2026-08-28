/**
 * Normalize common LaTeX shapes in agent/markdown text so Streamdown + KaTeX
 * (remark-math) can render them.
 *
 * remark-math only sees `$…$` / `$$…$$` (and we enable single-dollar). Models
 * often emit:
 * - bare TeX: `\pi_\theta`
 * - TeX delimiters: `\(...\)` / `\[...\]`
 * - block environments: `\begin{equation}...\end{equation}`
 * without dollar wrapping. Bare `_` also breaks GFM emphasis, so wrapping helps
 * both rendering and layout. KaTeX cannot render `equation` / `multline` /
 * `flalign` wrappers, so those are stripped while the inner math is kept.
 */

/** Fenced code, inline code, or existing dollar math — leave untouched. */
const PROTECTED =
	/```[\s\S]*?```|`[^`\n]*`|\$\$[\s\S]*?\$\$|\$(?:\\\$|[^$\n])+\$/g;

/** `\(...\)` inline and `\[...\]` display (non-greedy, allows nested `\cmd`). */
const TEX_DISPLAY = /\\\[([\s\S]*?)\\\]/g;
const TEX_INLINE = /\\\(([\s\S]*?)\\\)/g;

/** `\begin{env}...\end{env}` block environments (blogs, RSS excerpts). */
const TEX_ENV =
	/\\begin\{(equation\*?|multline\*?|flalign\*?|align\*?|alignat\*?|gather\*?|aligned|eqnarray\*?)\}([\s\S]*?)\\end\{\1\}/g;

/** Environments whose wrappers KaTeX cannot render (inner math is kept). */
const ENV_STRIP_WRAPPERS = /^(equation|multline|flalign)/;

/**
 * Bare command with at least one subscript/superscript, e.g. `\pi_\theta`,
 * `\alpha^{2}`, `x_\mathrm{t}`-style `\\mathrm{t}` scripts.
 */
const BARE_SCRIPTED =
	/(?<![$\\])\\[a-zA-Z]+(?:\{[^{}]*\})*(?:[_^](?:\{[^{}]*\}|\\[a-zA-Z]+(?:\{[^{}]*\})*|[A-Za-z0-9]+))+/g;

/**
 * Bare command with brace args only, e.g. `\frac{a}{b}`, `\mathcal{L}`.
 * Requires at least one `{…}` to avoid wrapping plain `\pi` or `\n`.
 */
const BARE_BRACED = /(?<![$\\])\\[a-zA-Z]+(?:\{[^{}]*\})+/g;

function transformMathRegion(text: string): string {
	if (!text.includes("\\")) return text;

	// Converted regions are stashed behind placeholders so later passes
	// (bare-command wrapping) cannot fragment their insides.
	const stash: string[] = [];
	const keep = (value: string) => {
		stash.push(value);
		return `\uE000${stash.length - 1}\uE000`;
	};

	let out = text.replace(TEX_ENV, (match, env: string, body: string) =>
		keep(ENV_STRIP_WRAPPERS.test(env) ? `$$${body.trim()}$$` : `$$${match}$$`),
	);
	out = out
		.replace(TEX_DISPLAY, (_m, body: string) => keep(`$$${body}$$`))
		.replace(TEX_INLINE, (_m, body: string) => keep(`$${body}$`));

	out = out.replace(BARE_SCRIPTED, (match) => keep(`$${match}$`));
	out = out.replace(BARE_BRACED, (match) => keep(`$${match}$`));

	return out.replace(/\uE000(\d+)\uE000/g, (_m, i: string) => stash[Number(i)]);
}

/** Prepare markdown so KaTeX can render common agent LaTeX forms. */
export function normalizeMarkdownMath(source: string): string {
	if (!source?.includes("\\")) return source;

	// Older cached markdown may still carry KaTeX-unsupported env
	// wrappers inside $$…$$; $$ regions are protected below, so unwrap first.
	const unwrapped = source.replace(
		/\$\$\s*\\begin\{(equation\*?|multline\*?|flalign\*?)\}([\s\S]*?)\\end\{\1\}\s*\$\$/g,
		(_m, _env: string, body: string) => `$$${body.trim()}$$`,
	);

	let result = "";
	let last = 0;
	for (const match of unwrapped.matchAll(PROTECTED)) {
		const start = match.index ?? 0;
		if (start > last) {
			result += transformMathRegion(unwrapped.slice(last, start));
		}
		result += match[0];
		last = start + match[0].length;
	}
	if (last < unwrapped.length) {
		result += transformMathRegion(unwrapped.slice(last));
	}
	return result;
}
