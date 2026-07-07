import {
	getWebFile,
	isWebDirectory,
	listWebFileEntriesUnderPath,
} from './web-file-store.js'
import { buildWebSendMetadataForPaths } from './web-send-metadata.js'
import { WebPreviewError } from './web-preview-error.js'

export type WebSendEntryType = 'file' | 'directory' | 'collection'

export function webSendEntryType(paths: string[]): WebSendEntryType {
	if (paths.length > 1) {
		return 'collection'
	}
	if (paths.length === 1 && isWebDirectory(paths[0])) {
		return 'directory'
	}
	return 'file'
}

export async function collectWebSendPayload(paths: string[]): Promise<{
	names: string[]
	bytesList: Uint8Array[]
	entryType: WebSendEntryType
	metadataJson: string
}> {
	const names: string[] = []
	const bytesList: Uint8Array[] = []

	for (const path of paths) {
		if (isWebDirectory(path)) {
			const entries = listWebFileEntriesUnderPath(path)
			if (entries.length === 0) {
				throw new WebPreviewError(
					`Folder "${path}" has no files to share. Please choose the folder again.`
				)
			}

			for (const entry of entries) {
				names.push(entry.path)
				bytesList.push(new Uint8Array(await entry.file.arrayBuffer()))
			}
			continue
		}

		const file = getWebFile(path)
		if (!file) {
			throw new WebPreviewError(
				'Selected file is no longer available. Please choose the file again.'
			)
		}

		names.push(path)
		bytesList.push(new Uint8Array(await file.arrayBuffer()))
	}

	if (names.length === 0) {
		throw new Error('No files selected')
	}

	return {
		names,
		bytesList,
		entryType: webSendEntryType(paths),
		metadataJson: buildWebSendMetadataForPaths(paths),
	}
}
