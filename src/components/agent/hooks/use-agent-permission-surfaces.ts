/**
 * Interactive ACP surfaces: permission requests (ask mode), Codex
 * request_user_input elicitation, Grok ask_user_question, and the
 * tool-shaped ask promoted by the runtime. Each registers in the overlay
 * stack so Esc dismisses (and cancels) the topmost one.
 */
import {
	type Dispatch,
	type SetStateAction,
	useEffect,
	useRef,
	useState,
} from "react";
import { useOverlayRegistration } from "@/hooks/use-overlay-registration";
import {
	type AskUserRequest,
	type ElicitationRequest,
	type PermissionRequest,
	respondAskUser,
	respondElicitation,
	respondPermission,
} from "@/lib/agent";
import type { ToolAskUserRequest } from "@/lib/agent/chat-state";
import { isTauri } from "@/lib/core/tauri";

export type UseAgentPermissionSurfacesOptions = {
	toolAskUserRequest: ToolAskUserRequest | null;
	setToolAskUserRequest: Dispatch<SetStateAction<ToolAskUserRequest | null>>;
};

export type AgentPermissionSurfaces = {
	permissionRequest: PermissionRequest | null;
	setPermissionRequest: Dispatch<SetStateAction<PermissionRequest | null>>;
	elicitationRequest: ElicitationRequest | null;
	setElicitationRequest: Dispatch<SetStateAction<ElicitationRequest | null>>;
	askUserRequest: AskUserRequest | null;
	setAskUserRequest: Dispatch<SetStateAction<AskUserRequest | null>>;
};

export function useAgentPermissionSurfaces({
	toolAskUserRequest,
	setToolAskUserRequest,
}: UseAgentPermissionSurfacesOptions): AgentPermissionSurfaces {
	// Forward ACP permission requests (ask mode) to the user for an explicit decision.
	const [permissionRequest, setPermissionRequest] =
		useState<PermissionRequest | null>(null);
	// Codex Plan-mode request_user_input → form elicitation.
	const [elicitationRequest, setElicitationRequest] =
		useState<ElicitationRequest | null>(null);
	// Grok `_x.ai/ask_user_question` extension method.
	const [askUserRequest, setAskUserRequest] = useState<AskUserRequest | null>(
		null,
	);

	const permissionRequestRef = useRef(permissionRequest);
	permissionRequestRef.current = permissionRequest;
	useOverlayRegistration("agent-permission", permissionRequest !== null, () => {
		const req = permissionRequestRef.current;
		if (!req) return;
		void respondPermission(req.requestId, null);
		setPermissionRequest(null);
	});

	const elicitationRequestRef = useRef(elicitationRequest);
	elicitationRequestRef.current = elicitationRequest;
	useOverlayRegistration(
		"agent-elicitation",
		elicitationRequest !== null,
		() => {
			const req = elicitationRequestRef.current;
			if (!req) return;
			void respondElicitation({
				requestId: req.requestId,
				action: "cancel",
			});
			setElicitationRequest(null);
		},
	);

	const askUserRequestRef = useRef(askUserRequest);
	askUserRequestRef.current = askUserRequest;
	useOverlayRegistration("agent-ask-user", askUserRequest !== null, () => {
		const req = askUserRequestRef.current;
		if (!req) return;
		void respondAskUser({
			requestId: req.requestId,
			action: "cancel",
		});
		setAskUserRequest(null);
	});

	const toolAskUserRequestRef = useRef(toolAskUserRequest);
	toolAskUserRequestRef.current = toolAskUserRequest;
	useOverlayRegistration(
		"agent-tool-ask-user",
		toolAskUserRequest !== null,
		() => {
			setToolAskUserRequest(null);
		},
	);

	useEffect(() => {
		if (!isTauri()) return;
		let unsub: (() => void) | undefined;
		let cancelled = false;
		void (async () => {
			const { listen } = await import("@tauri-apps/api/event");
			if (cancelled) return;
			unsub = await listen<PermissionRequest>(
				"agent:permission-request",
				({ payload }) => setPermissionRequest(payload),
			);
		})();
		return () => {
			cancelled = true;
			unsub?.();
		};
	}, []);

	useEffect(() => {
		if (!isTauri()) return;
		let unsub: (() => void) | undefined;
		let cancelled = false;
		void (async () => {
			const { listen } = await import("@tauri-apps/api/event");
			if (cancelled) return;
			unsub = await listen<ElicitationRequest>(
				"agent:elicitation-request",
				({ payload }) => {
					// Prefer host elicitation over tool-card promote.
					setToolAskUserRequest(null);
					setElicitationRequest(payload);
				},
			);
		})();
		return () => {
			cancelled = true;
			unsub?.();
		};
	}, [setToolAskUserRequest]);

	useEffect(() => {
		if (!isTauri()) return;
		let unsub: (() => void) | undefined;
		let cancelled = false;
		void (async () => {
			const { listen } = await import("@tauri-apps/api/event");
			if (cancelled) return;
			unsub = await listen<AskUserRequest>(
				"agent:ask-user-request",
				({ payload }) => {
					// Grok ext is the authoritative respond path; drop tool-promote duplicate.
					setToolAskUserRequest(null);
					setAskUserRequest(payload);
				},
			);
		})();
		return () => {
			cancelled = true;
			unsub?.();
		};
	}, [setToolAskUserRequest]);

	return {
		permissionRequest,
		setPermissionRequest,
		elicitationRequest,
		setElicitationRequest,
		askUserRequest,
		setAskUserRequest,
	};
}
