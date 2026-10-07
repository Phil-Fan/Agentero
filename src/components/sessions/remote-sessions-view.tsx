/**
 * 「远端会话」panel — virtual workspace tab at the same level as 广场.
 *
 * Lists Agentero nodes from the local registry, polls each node's A2A tasks,
 * submits `run` / `agent` tasks over the Host commands, and shows task detail
 * (artifacts) beside the roster.
 */

import {
	Loader2,
	Play,
	Plus,
	RefreshCw,
	Server,
	Trash2,
	X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";
import { commands } from "@/lib/core/bindings";
import { callResult } from "@/lib/core/ipc";
import { notifyError } from "@/lib/core/notify";
import { cn } from "@/lib/core/utils";

type RegisteredNode = {
	nodeId: string;
	shortId: string;
	name: string;
	transport: string;
	url: string | null;
	sshHost: string | null;
	publicKey: string | null;
	lastSeen: string;
};

type NodeRegistry = { version: number; nodes: RegisteredNode[] };

type A2ATask = {
	id: string;
	contextId: string;
	status: {
		state: string;
		message?: { parts: Array<{ kind: string; text?: string }> };
	};
	artifacts?: Array<{
		name?: string;
		parts: Array<{ kind: string; text?: string }>;
	}>;
};

type TaskSummary = {
	id: string;
	contextId: string;
	state: string;
	timestamp?: string;
};

const TERMINAL_STATES = new Set([
	"completed",
	"failed",
	"canceled",
	"rejected",
]);

function parseJson<T>(raw: string): T {
	return JSON.parse(raw) as T;
}

function textOf(
	parts: Array<{ kind: string; text?: string }> | undefined,
): string {
	return (parts ?? [])
		.filter((p) => p.kind === "text" && p.text)
		.map((p) => p.text)
		.join("\n");
}

export function RemoteSessionsView({ className }: { className?: string }) {
	const { t } = useTranslation("sidebar");
	const [nodes, setNodes] = useState<RegisteredNode[]>([]);
	const [selected, setSelected] = useState<string | null>(null);
	const [tasks, setTasks] = useState<TaskSummary[]>([]);
	const [detail, setDetail] = useState<A2ATask | null>(null);

	const [newTarget, setNewTarget] = useState("");
	const [newName, setNewName] = useState("");
	const [pairCode, setPairCode] = useState("");
	const [input, setInput] = useState("");
	const [cwd, setCwd] = useState("");
	const [skill, setSkill] = useState<"run" | "agent">("run");
	const [busy, setBusy] = useState(false);

	const refreshNodes = useCallback(async () => {
		try {
			const raw = await callResult(() => commands.nodeList());
			const registry = parseJson<NodeRegistry>(raw);
			setNodes(registry.nodes ?? []);
			setSelected((prev) => {
				if (prev && registry.nodes.some((n) => n.nodeId === prev)) return prev;
				return registry.nodes[0]?.nodeId ?? null;
			});
		} catch (e) {
			notifyError(String(e));
		}
	}, []);

	const node = useMemo(
		() => nodes.find((n) => n.nodeId === selected) ?? null,
		[nodes, selected],
	);
	const target = node?.url ?? null;

	const refreshTasks = useCallback(async () => {
		if (!target) {
			setTasks([]);
			return;
		}
		try {
			const raw = await callResult(() => commands.nodeTasks(target, null));
			setTasks(parseJson<TaskSummary[]>(raw));
		} catch {
			// Transient (node offline): keep the last snapshot.
		}
	}, [target]);

	useEffect(() => {
		void refreshNodes();
	}, [refreshNodes]);

	useEffect(() => {
		if (!target) {
			setTasks([]);
			return;
		}
		setDetail(null);
		void refreshTasks();
		const id = window.setInterval(() => void refreshTasks(), 2000);
		return () => window.clearInterval(id);
	}, [target, refreshTasks]);

	const onAdd = useCallback(async () => {
		const value = newTarget.trim();
		if (!value) return;
		setBusy(true);
		try {
			await callResult(() => commands.nodeAdd(value, newName.trim() || null));
			setNewTarget("");
			setNewName("");
			await refreshNodes();
		} catch (e) {
			notifyError(String(e));
		} finally {
			setBusy(false);
		}
	}, [newTarget, newName, refreshNodes]);

	const onForget = useCallback(async () => {
		if (!node) return;
		setBusy(true);
		try {
			await callResult(() => commands.nodeRemove(node.nodeId));
			setDetail(null);
			await refreshNodes();
		} catch (e) {
			notifyError(String(e));
		} finally {
			setBusy(false);
		}
	}, [node, refreshNodes]);

	const onPair = useCallback(async () => {
		if (!target || !pairCode.trim()) return;
		setBusy(true);
		try {
			await callResult(() => commands.nodePair(target, pairCode.trim(), null));
			setPairCode("");
			await refreshNodes();
		} catch (e) {
			notifyError(String(e));
		} finally {
			setBusy(false);
		}
	}, [target, pairCode, refreshNodes]);

	const onSubmit = useCallback(async () => {
		if (!target || !input.trim()) return;
		setBusy(true);
		try {
			await callResult(() =>
				commands.nodeSubmit(target, input.trim(), skill, cwd.trim() || null),
			);
			setInput("");
			await refreshTasks();
		} catch (e) {
			notifyError(String(e));
		} finally {
			setBusy(false);
		}
	}, [target, input, skill, cwd, refreshTasks]);

	const openTask = useCallback(
		async (id: string) => {
			if (!target) return;
			try {
				const raw = await callResult(() => commands.nodeTask(target, id));
				setDetail(parseJson<A2ATask>(raw));
			} catch (e) {
				notifyError(String(e));
			}
		},
		[target],
	);

	const cancelTask = useCallback(
		async (id: string) => {
			if (!target) return;
			try {
				await callResult(() => commands.nodeCancel(target, id));
				await refreshTasks();
			} catch (e) {
				notifyError(String(e));
			}
		},
		[target, refreshTasks],
	);

	return (
		<div className={cn("flex min-h-0 flex-1 text-sm", className)}>
			{/* Roster */}
			<div className="flex w-64 shrink-0 flex-col border-border border-r">
				<div className="flex items-center justify-between px-3 py-2">
					<span className="font-medium">{t("remoteSessions.nodes")}</span>
					<Button
						variant="ghost"
						size="icon"
						className="size-7"
						aria-label={t("remoteSessions.refresh")}
						onClick={() => void refreshNodes()}
					>
						<RefreshCw className="size-4" />
					</Button>
				</div>
				<div className="agentero-scroll min-h-0 flex-1 overflow-y-auto px-1">
					{nodes.length === 0 ? (
						<p className="px-3 py-2 text-muted-foreground text-xs">
							{t("remoteSessions.empty")}
						</p>
					) : (
						nodes.map((n) => (
							<button
								type="button"
								key={n.nodeId}
								onClick={() => setSelected(n.nodeId)}
								className={cn(
									"flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left",
									n.nodeId === selected
										? "bg-accent text-accent-foreground"
										: "hover:bg-accent/50",
								)}
							>
								<Server className="size-4 shrink-0 text-muted-foreground" />
								<span className="min-w-0 flex-1 truncate">
									{n.name || n.shortId}
								</span>
							</button>
						))
					)}
				</div>
				<div className="flex flex-col gap-1 border-border border-t p-2">
					<input
						value={newTarget}
						onChange={(e) => setNewTarget(e.target.value)}
						placeholder={t("remoteSessions.addPlaceholder")}
						className="h-8 rounded-md border border-border bg-transparent px-2 text-xs outline-none focus:ring-1 focus:ring-ring"
					/>
					<input
						value={newName}
						onChange={(e) => setNewName(e.target.value)}
						placeholder={t("remoteSessions.namePlaceholder")}
						className="h-8 rounded-md border border-border bg-transparent px-2 text-xs outline-none focus:ring-1 focus:ring-ring"
					/>
					<Button
						size="sm"
						variant="secondary"
						disabled={busy || !newTarget.trim()}
						onClick={() => void onAdd()}
					>
						<Plus className="size-4" />
						{t("remoteSessions.add")}
					</Button>
				</div>
			</div>

			{/* Detail */}
			<div className="flex min-h-0 min-w-0 flex-1 flex-col">
				{!node ? (
					<div className="flex flex-1 items-center justify-center text-muted-foreground text-xs">
						{t("remoteSessions.selectNode")}
					</div>
				) : (
					<>
						<div className="flex items-center gap-2 border-border border-b px-3 py-2">
							<Server className="size-4 text-muted-foreground" />
							<span className="font-medium">{node.name}</span>
							<span className="truncate text-muted-foreground text-xs">
								{node.url}
							</span>
							<div className="ml-auto flex items-center gap-1">
								<input
									value={pairCode}
									onChange={(e) => setPairCode(e.target.value)}
									placeholder={t("remoteSessions.pairCode")}
									className="h-7 w-28 rounded-md border border-border bg-transparent px-2 text-xs outline-none focus:ring-1 focus:ring-ring"
								/>
								<Button
									size="sm"
									variant="ghost"
									disabled={busy || !pairCode.trim()}
									onClick={() => void onPair()}
								>
									{t("remoteSessions.pair")}
								</Button>
								<Button
									variant="ghost"
									size="icon"
									className="size-7"
									aria-label={t("remoteSessions.forget")}
									disabled={busy}
									onClick={() => void onForget()}
								>
									<Trash2 className="size-4" />
								</Button>
							</div>
						</div>

						<div className="min-h-0 flex-1 overflow-hidden">
							<div className="agentero-scroll h-full overflow-y-auto">
								{tasks.length === 0 ? (
									<p className="px-3 py-3 text-muted-foreground text-xs">
										{t("remoteSessions.noTasks")}
									</p>
								) : (
									tasks.map((task) => (
										<div
											key={task.id}
											className="flex items-center gap-2 border-border/60 border-b px-3 py-1.5"
										>
											<span
												className={cn(
													"size-2 shrink-0 rounded-full",
													task.state === "completed" && "bg-emerald-500",
													task.state === "failed" && "bg-destructive",
													task.state === "canceled" && "bg-muted-foreground",
													!TERMINAL_STATES.has(task.state) && "bg-primary",
												)}
											/>
											<button
												type="button"
												onClick={() => void openTask(task.id)}
												className="min-w-0 flex-1 truncate text-left font-mono text-xs hover:underline"
											>
												{task.id.slice(0, 8)}
											</button>
											<span className="text-muted-foreground text-xs">
												{task.state}
											</span>
											{!TERMINAL_STATES.has(task.state) ? (
												<Button
													variant="ghost"
													size="icon"
													className="size-6"
													aria-label={t("remoteSessions.cancel")}
													onClick={() => void cancelTask(task.id)}
												>
													<X className="size-3.5" />
												</Button>
											) : null}
										</div>
									))
								)}

								{detail ? (
									<div className="border-border border-t bg-muted/30 p-3">
										<div className="mb-1 flex items-center justify-between">
											<span className="font-mono text-xs">
												{detail.id.slice(0, 8)} · {detail.status.state}
											</span>
											<Button
												variant="ghost"
												size="icon"
												className="size-6"
												aria-label={t("remoteSessions.close")}
												onClick={() => setDetail(null)}
											>
												<X className="size-3.5" />
											</Button>
										</div>
										{(detail.artifacts ?? []).map((artifact, i) => (
											<pre
												// biome-ignore lint/suspicious/noArrayIndexKey: artifacts have no id
												key={i}
												className="agentero-scroll mt-1 max-h-64 overflow-auto whitespace-pre-wrap break-words rounded bg-background p-2 text-xs"
											>
												{artifact.name ? `${artifact.name}\n` : ""}
												{textOf(artifact.parts)}
											</pre>
										))}
									</div>
								) : null}
							</div>
						</div>

						<div className="flex items-center gap-2 border-border border-t p-2">
							<button
								type="button"
								onClick={() => setSkill("run")}
								className={cn(
									"rounded px-2 py-1 text-xs",
									skill === "run"
										? "bg-accent text-accent-foreground"
										: "text-muted-foreground",
								)}
							>
								{t("remoteSessions.skillRun")}
							</button>
							<button
								type="button"
								onClick={() => setSkill("agent")}
								className={cn(
									"rounded px-2 py-1 text-xs",
									skill === "agent"
										? "bg-accent text-accent-foreground"
										: "text-muted-foreground",
								)}
							>
								{t("remoteSessions.skillAgent")}
							</button>
							<input
								value={cwd}
								onChange={(e) => setCwd(e.target.value)}
								placeholder={t("remoteSessions.cwdPlaceholder")}
								className="h-7 w-40 rounded-md border border-border bg-transparent px-2 text-xs outline-none focus:ring-1 focus:ring-ring"
							/>
							<input
								value={input}
								onChange={(e) => setInput(e.target.value)}
								onKeyDown={(e) => {
									if (e.key === "Enter") void onSubmit();
								}}
								placeholder={t("remoteSessions.inputPlaceholder")}
								className="h-8 min-w-0 flex-1 rounded-md border border-border bg-transparent px-2 text-xs outline-none focus:ring-1 focus:ring-ring"
							/>
							<Button
								size="icon"
								className="size-8"
								disabled={busy || !input.trim()}
								aria-label={t("remoteSessions.submit")}
								onClick={() => void onSubmit()}
							>
								{busy ? (
									<Loader2 className="size-4 animate-spin" />
								) : (
									<Play className="size-4" />
								)}
							</Button>
						</div>
					</>
				)}
			</div>
		</div>
	);
}
