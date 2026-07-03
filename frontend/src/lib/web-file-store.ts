/** In-memory File handles for browser send (paths are display keys, not filesystem paths). */

const filesByPath = new Map<string, File>()
const directoryPaths = new Set<string>()

export function registerWebFile(path: string, file: File): void {
	filesByPath.set(path, file)
	directoryPaths.delete(path)
}

export function registerWebDirectory(path: string): void {
	directoryPaths.add(path)
	filesByPath.delete(path)
}

export function getWebFile(path: string): File | undefined {
	return filesByPath.get(path)
}

export function isWebDirectory(path: string): boolean {
	return directoryPaths.has(path)
}

export function clearWebFile(path: string): void {
	filesByPath.delete(path)
	directoryPaths.delete(path)
}

export function clearWebFiles(): void {
	filesByPath.clear()
	directoryPaths.clear()
}

export function webFilePathKey(file: File): string {
	const relative = (file as File & { webkitRelativePath?: string })
		.webkitRelativePath
	return relative && relative.length > 0 ? relative : file.name
}

/** All registered files whose path equals `path` or lives under `path/`. */
export function listWebFilesUnderPath(path: string): File[] {
	const prefix = `${path}/`
	const files: File[] = []
	for (const [filePath, file] of filesByPath) {
		if (filePath === path || filePath.startsWith(prefix)) {
			files.push(file)
		}
	}
	return files
}
