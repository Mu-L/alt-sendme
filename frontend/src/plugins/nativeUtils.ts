import { IS_TAURI } from '@/lib/platform'
import { invoke, openDialog, pickDownloadDirectory } from '@/lib/platform-api'

export type DownloadFolderSelectionResponse = {
	uri: string
	path: string
}

export type CopyProgress = {
	totalBytes: string
	progress: number
	cachedPath?: string
}

export class FileSelectedHandler {
	private channelId: string
	private active = true

	constructor(channelId: string) {
		this.channelId = channelId
	}

	public async cancelJob() {
		if (!this.active) return
		await invoke<void>('plugin:native-utils|cancel_job', {
			job: { channelId: this.channelId },
		})
		this.active = false
	}
}

export async function selectDownloadFolder(): Promise<DownloadFolderSelectionResponse | null> {
	if (!IS_TAURI) {
		const path = await pickDownloadDirectory()
		if (!path) return null
		return { uri: path, path }
	}

	return await invoke<DownloadFolderSelectionResponse>(
		'plugin:native-utils|select_download_folder'
	)
}

export async function selectSendDocument(
	onStart: (path: string, size: BigInt) => void,
	onEvent: (event: CopyProgress) => void,
	onComplete: (path: string) => void
): Promise<FileSelectedHandler | null> {
	if (!IS_TAURI) {
		const selected = await openDialog({ multiple: true, directory: false })
		if (!selected) return null
		const paths = Array.isArray(selected) ? selected : [selected]
		for (const path of paths) {
			onStart(path, BigInt(0))
			onComplete(path)
		}
		return null
	}

	const { Channel } = await import('@tauri-apps/api/core')
	const channel = new Channel<CopyProgress>()
	channel.onmessage = (event: CopyProgress) => {
		if (event.progress === 0 && event.cachedPath) {
			onStart(event.cachedPath, BigInt(event.totalBytes))
		} else if (event.progress === 1 && event.cachedPath) {
			onComplete(event.cachedPath)
		} else {
			onEvent(event)
		}
	}
	const response = await invoke<boolean | undefined>(
		'plugin:native-utils|select_send_document',
		{
			channel: channel,
		}
	)
	if (!response) return null
	return new FileSelectedHandler(String(channel.id))
}

export async function selectSendFolder(
	onStart: (path: string, size: BigInt) => void,
	onEvent: (event: CopyProgress) => void,
	onComplete: (path: string) => void
): Promise<FileSelectedHandler | null> {
	if (!IS_TAURI) {
		const selected = await openDialog({ multiple: false, directory: true })
		if (!selected) return null
		const path = Array.isArray(selected) ? selected[0] : selected
		if (!path) return null
		onStart(path, BigInt(0))
		onComplete(path)
		return null
	}

	const { Channel } = await import('@tauri-apps/api/core')
	const channel = new Channel<CopyProgress>()
	channel.onmessage = (event: CopyProgress) => {
		if (event.progress === 0 && event.cachedPath) {
			onStart(event.cachedPath, BigInt(event.totalBytes))
		} else if (event.progress === 1 && event.cachedPath) {
			onComplete(event.cachedPath)
		} else {
			onEvent(event)
		}
	}
	const response = await invoke<boolean>(
		'plugin:native-utils|select_send_folder',
		{
			channel: channel,
		}
	)
	if (!response) return null
	return new FileSelectedHandler(String(channel.id))
}
