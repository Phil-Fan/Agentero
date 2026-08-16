import { emit } from "@/lib/lifecycle/bus";
import type { LifecycleEvent, LifecycleEventMap } from "@/lib/lifecycle/events";

const WIRE_EVENTS = [
	"paper:imported",
	"paper:assets-ready",
	"job:completed",
	"job:failed",
] as const satisfies readonly LifecycleEvent[];

let bridgePromise: Promise<() => void> | null = null;

/** Forwards Tauri wire lifecycle events into the frontend bus. Idempotent. */
export function initLifecycleBridge(): Promise<() => void> {
	if (bridgePromise) return bridgePromise;
	bridgePromise = (async () => {
		const { listen } = await import("@tauri-apps/api/event");
		const unlistens = await Promise.all(
			WIRE_EVENTS.map((event) =>
				listen<LifecycleEventMap[typeof event]>(event, (e) => {
					void emit(event, e.payload);
				}),
			),
		);
		return () => {
			for (const unlisten of unlistens) unlisten();
			bridgePromise = null;
		};
	})();
	return bridgePromise;
}
