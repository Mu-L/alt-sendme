type WebEventHandler = (event: { payload: unknown }) => void

const listeners = new Map<string, Set<WebEventHandler>>()

export function dispatchWebEvent(eventName: string, payload?: string): void {
	const handlers = listeners.get(eventName)
	if (!handlers) return

	const event = { payload: payload ?? null }
	for (const handler of handlers) {
		handler(event)
	}
}

export function subscribeWebEvent(
	eventName: string,
	handler: WebEventHandler
): () => void {
	let handlers = listeners.get(eventName)
	if (!handlers) {
		handlers = new Set()
		listeners.set(eventName, handlers)
	}
	handlers.add(handler)

	return () => {
		handlers?.delete(handler)
		if (handlers && handlers.size === 0) {
			listeners.delete(eventName)
		}
	}
}
