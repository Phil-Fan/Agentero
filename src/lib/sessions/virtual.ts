import i18n from "@/i18n";

/**
 * Virtual path for the「远端会话」panel. Never a filesystem path; it lives in
 * the same space as Library / Recycle Bin / Plaza (`agentero:*`).
 */
export const SESSIONS_VIRTUAL_PATH = "agentero:sessions";

export function isSessionsVirtualPath(
	path: string | null | undefined,
): boolean {
	return path === SESSIONS_VIRTUAL_PATH;
}

export function sessionsTitle(): string {
	return i18n.t("sidebar:remoteSessions.title");
}
