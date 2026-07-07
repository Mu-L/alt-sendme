import {
	registerWebDirectory,
	registerWebFile,
	webFilePathKey,
} from './web-file-store'

export type WebDropResult = {
	paths: string[]
	pathType?: 'file' | 'directory'
}

function readFileEntry(entry: FileSystemFileEntry): Promise<File> {
	return new Promise((resolve, reject) => {
		entry.file(resolve, reject)
	})
}

function readDirectoryEntries(
	entry: FileSystemDirectoryEntry
): Promise<FileSystemEntry[]> {
	return new Promise((resolve, reject) => {
		const reader = entry.createReader()
		const allEntries: FileSystemEntry[] = []

		const readBatch = () => {
			reader.readEntries((entries) => {
				if (entries.length === 0) {
					resolve(allEntries)
					return
				}
				allEntries.push(...entries)
				readBatch()
			}, reject)
		}

		readBatch()
	})
}

async function readEntryRecursively(
	entry: FileSystemEntry,
	basePath: string
): Promise<Array<{ path: string; file: File }>> {
	if (entry.isFile) {
		const file = await readFileEntry(entry as FileSystemFileEntry)
		const path = basePath ? `${basePath}/${entry.name}` : entry.name
		return [{ path, file }]
	}

	if (entry.isDirectory) {
		const dirPath = basePath ? `${basePath}/${entry.name}` : entry.name
		const children = await readDirectoryEntries(
			entry as FileSystemDirectoryEntry
		)
		const nested = await Promise.all(
			children.map((child) => readEntryRecursively(child, dirPath))
		)
		return nested.flat()
	}

	return []
}

function registerPlainFiles(files: File[]): string[] {
	const paths: string[] = []
	for (const file of files) {
		const path = webFilePathKey(file)
		registerWebFile(path, file)
		paths.push(path)
	}
	return paths
}

function resolvePathType(
	paths: string[],
	hasDirectory: boolean,
	hasFile: boolean
): WebDropResult['pathType'] {
	if (paths.length !== 1) {
		return hasDirectory && !hasFile ? 'directory' : undefined
	}
	if (hasDirectory && !hasFile) {
		return 'directory'
	}
	if (hasFile && !hasDirectory) {
		return 'file'
	}
	return undefined
}

export async function processWebDataTransfer(
	dataTransfer: DataTransfer
): Promise<WebDropResult> {
	const items = Array.from(dataTransfer.items)

	if (items.length === 0) {
		const paths = registerPlainFiles(Array.from(dataTransfer.files))
		return {
			paths,
			pathType: paths.length === 1 ? 'file' : undefined,
		}
	}

	const topLevelPaths: string[] = []
	let hasDirectory = false
	let hasFile = false

	for (const item of items) {
		if (item.kind !== 'file') {
			continue
		}

		const entry = item.webkitGetAsEntry?.()
		if (!entry) {
			const file = item.getAsFile()
			if (!file) {
				continue
			}
			const path = webFilePathKey(file)
			registerWebFile(path, file)
			topLevelPaths.push(path)
			hasFile = true
			continue
		}

		if (entry.isDirectory) {
			hasDirectory = true
			const folderName = entry.name
			registerWebDirectory(folderName)
			topLevelPaths.push(folderName)

			const nested = await readEntryRecursively(entry, '')
			for (const { path, file } of nested) {
				registerWebFile(path, file)
			}
			continue
		}

		if (entry.isFile) {
			hasFile = true
			const file = await readFileEntry(entry as FileSystemFileEntry)
			const path = webFilePathKey(file)
			registerWebFile(path, file)
			topLevelPaths.push(path)
		}
	}

	return {
		paths: topLevelPaths,
		pathType: resolvePathType(topLevelPaths, hasDirectory, hasFile),
	}
}
