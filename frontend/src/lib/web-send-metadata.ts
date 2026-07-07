import {
	getWebFile,
	isWebDirectory,
	listWebFilesUnderPath,
} from './web-file-store.js'

type WebFilePreviewItem = {
	file_name: string
	size: number
	thumbnail: null
	mime_type: string
}

type WebFileMetadata = {
	file_name: string
	item_count: number
	size: number
	thumbnail: null
	mime_type: string
	items?: WebFilePreviewItem[]
}

function webMimeType(file: File): string {
	return file.type || 'application/octet-stream'
}

function basename(path: string): string {
	const segments = path.split('/').filter(Boolean)
	return segments[segments.length - 1] ?? path
}

function dedupName(name: string, seen: Map<string, number>): string {
	const count = seen.get(name) ?? 0
	seen.set(name, count + 1)
	if (count === 0) {
		return name
	}
	const dot = name.lastIndexOf('.')
	if (dot > 0) {
		return `${name.slice(0, dot)} (${count})${name.slice(dot)}`
	}
	return `${name} (${count})`
}

function buildPreviewItems(paths: string[]): WebFilePreviewItem[] {
	const seen = new Map<string, number>()
	const items: WebFilePreviewItem[] = []

	for (const path of paths) {
		if (isWebDirectory(path)) {
			items.push({
				file_name: dedupName(basename(path), seen),
				size: listWebFilesUnderPath(path).reduce(
					(total, file) => total + file.size,
					0
				),
				thumbnail: null,
				mime_type: 'inode/directory',
			})
			continue
		}

		const file = getWebFile(path)
		if (!file) {
			continue
		}

		items.push({
			file_name: dedupName(basename(path), seen),
			size: file.size,
			thumbnail: null,
			mime_type: webMimeType(file),
		})
	}

	return items.sort((a, b) => a.file_name.localeCompare(b.file_name))
}

function totalSizeForPaths(paths: string[]): number {
	let total = 0
	for (const path of paths) {
		if (isWebDirectory(path)) {
			for (const file of listWebFilesUnderPath(path)) {
				total += file.size
			}
			continue
		}
		total += getWebFile(path)?.size ?? 0
	}
	return total
}

/** Web send metadata for a single in-memory file (no thumbnail generation). */
export function buildWebSendMetadataForFile(file: File): string {
	const metadata: WebFileMetadata = {
		file_name: file.name,
		item_count: 1,
		size: file.size,
		mime_type: webMimeType(file),
		thumbnail: null,
	}
	return JSON.stringify(metadata)
}

/**
 * Build send metadata for one or more web paths (files or folders).
 * Mirrors desktop `build_send_metadata` but never attaches thumbnails.
 */
export function buildWebSendMetadataForPaths(paths: string[]): string {
	if (paths.length === 0) {
		throw new Error('No paths provided')
	}

	if (paths.length === 1) {
		const path = paths[0]
		if (isWebDirectory(path)) {
			const metadata: WebFileMetadata = {
				file_name: basename(path),
				item_count: 1,
				size: totalSizeForPaths([path]),
				mime_type: 'inode/directory',
				thumbnail: null,
			}
			return JSON.stringify(metadata)
		}

		const file = getWebFile(path)
		if (!file) {
			throw new Error('Selected file is no longer available')
		}
		return buildWebSendMetadataForFile(file)
	}

	const items = buildPreviewItems(paths)
	const metadata: WebFileMetadata = {
		file_name: basename(paths[0]),
		item_count: items.length,
		size: totalSizeForPaths(paths),
		mime_type: 'application/x-iroh-collection',
		thumbnail: null,
		items,
	}
	return JSON.stringify(metadata)
}
