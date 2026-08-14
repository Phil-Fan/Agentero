/**
 * 广场（Plaza）— external discovery sources.
 *
 * Virtual tree/tab paths only; nothing here ever touches disk. Adding a source
 * is a single {@link PLAZA_SOURCES} entry — the sidebar node, its child row, the
 * home page card and the center panel all derive from this registry.
 *
 * @see docs/development/plaza.md
 */

import type { ParseKeys } from "i18next";
import { Sparkles } from "lucide-react";
import type { ComponentType, SVGProps } from "react";
import { CoolPapersIcon } from "@/components/icons/cool-papers-icon";
import i18n from "@/i18n";

/** Virtual tree/tab path for the Plaza parent node. */
export const PLAZA_VIRTUAL_PATH = "agentero:plaza";

export type PlazaSource = {
	id: string;
	/** `agentero:plaza/<id>` — virtual, never a filesystem path. */
	path: string;
	label: string;
	/**
	 * Sidebar-namespace i18n key for the one line shown on the Plaza home card.
	 * Stored as a key (translated at render) so it follows language switches.
	 */
	description: ParseKeys<"sidebar">;
	/**
	 * Canonical public site: used for "open in browser", and embedded directly
	 * when there is no proxy. `null` with no {@link panel} is a placeholder.
	 */
	url: string | null;
	/** Native plaza panel (not an iframe). */
	panel?: "skills";
	/**
	 * Host proxy scheme origin used for embedding. A cross-origin frame cannot
	 * retarget the site's `target="_blank"` links or report its navigations, so
	 * sources meant for in-frame browsing are served under our own scheme.
	 * `null` embeds {@link url} as-is.
	 */
	embedOrigin: (() => string) | null;
	icon: ComponentType<SVGProps<SVGSVGElement>>;
};

function sourcePath(id: string): string {
	return `${PLAZA_VIRTUAL_PATH}/${id}`;
}

/** Custom-scheme origin, spelled the way each platform's WebView expects. */
function schemeOrigin(scheme: string): string {
	return navigator.userAgent.includes("Windows")
		? `https://${scheme}.localhost`
		: `${scheme}://localhost`;
}

export const PLAZA_SOURCES: readonly PlazaSource[] = [
	{
		id: "cool-papers",
		path: sourcePath("cool-papers"),
		label: "Cool Papers",
		description: "plaza.arxivDescription",
		url: "https://papers.cool/",
		embedOrigin: () => schemeOrigin("agentero-coolpapers"),
		icon: CoolPapersIcon,
	},
	{
		id: "skills",
		path: sourcePath("skills"),
		label: "Skill picks",
		description: "plaza.skillsDescription",
		url: null,
		embedOrigin: null,
		panel: "skills",
		icon: Sparkles,
	},
];

/** True for the Plaza parent node and every source under this parent. */
export function isPlazaVirtualPath(path: string | null | undefined): boolean {
	if (!path) return false;
	return (
		path === PLAZA_VIRTUAL_PATH || path.startsWith(`${PLAZA_VIRTUAL_PATH}/`)
	);
}

export function isPlazaRootPath(path: string | null | undefined): boolean {
	return path === PLAZA_VIRTUAL_PATH;
}

/** The source owning this path, or `null` for the Plaza root / non-Plaza paths. */
export function plazaSourceForPath(
	path: string | null | undefined,
): PlazaSource | null {
	if (!path) return null;
	return PLAZA_SOURCES.find((source) => source.path === path) ?? null;
}

/** Tab title for any Plaza path. */
export function plazaTitleForPath(path: string): string {
	const source = plazaSourceForPath(path);
	if (source?.id === "skills") {
		return i18n.t("sidebar:plaza.skills.title");
	}
	return source?.label ?? i18n.t("sidebar:plaza.plaza");
}
