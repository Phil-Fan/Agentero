import { logger } from "@/lib/core/logger";
import type { LifecycleEvent, LifecycleEventMap } from "@/lib/lifecycle/events";

type Handler<E extends LifecycleEvent> = (
	payload: LifecycleEventMap[E],
) => void | Promise<void>;

const handlers = new Map<LifecycleEvent, Handler<LifecycleEvent>[]>();

export function on<E extends LifecycleEvent>(
	event: E,
	handler: Handler<E>,
): () => void {
	const list = handlers.get(event) ?? [];
	list.push(handler as Handler<LifecycleEvent>);
	handlers.set(event, list);
	return () => {
		const current = handlers.get(event);
		if (!current) return;
		const idx = current.indexOf(handler as Handler<LifecycleEvent>);
		if (idx >= 0) current.splice(idx, 1);
	};
}

/** Handlers run serially in registration order; ordering constraints are
 *  expressed by registration order, not priorities. */
export async function emit<E extends LifecycleEvent>(
	event: E,
	payload: LifecycleEventMap[E],
): Promise<void> {
	const list = handlers.get(event);
	if (!list) return;
	for (const handler of [...list]) {
		try {
			await handler(payload);
		} catch (e) {
			logger.error(`lifecycle handler failed event=${event}`, {
				error: e instanceof Error ? e.message : String(e),
			});
		}
	}
}
