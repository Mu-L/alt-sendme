export class WebPreviewError extends Error {
	constructor(
		message = 'File transfer is not available in the web preview. Use the desktop app to send and receive files.'
	) {
		super(message)
		this.name = 'WebPreviewError'
	}
}

export function isWebPreviewError(error: unknown): boolean {
	return error instanceof WebPreviewError
}

export function getWebPreviewErrorMessage(
	error: unknown,
	fallback: string
): string {
	if (error instanceof WebPreviewError) {
		const message = error.message?.trim()
		return message.length > 0 ? message : fallback
	}
	return fallback
}
