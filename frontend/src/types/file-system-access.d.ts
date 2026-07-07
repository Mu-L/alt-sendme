interface FileSystemHandlePermissionDescriptor {
	mode: 'read' | 'readwrite'
}

interface FileSystemDirectoryHandle {
	queryPermission(
		descriptor: FileSystemHandlePermissionDescriptor
	): Promise<PermissionState>
	requestPermission(
		descriptor: FileSystemHandlePermissionDescriptor
	): Promise<PermissionState>
	getFileHandle(
		name: string,
		options?: { create?: boolean }
	): Promise<FileSystemFileHandle>
	readonly name: string
}

interface FileSystemFileHandle {
	createWritable(): Promise<FileSystemWritableFileStream>
	readonly name: string
}

interface FileSystemWritableFileStream extends WritableStream {
	write(data: BufferSource | Blob | string): Promise<void>
	close(): Promise<void>
}

interface Window {
	showDirectoryPicker?(options?: {
		mode?: 'read' | 'readwrite'
	}): Promise<FileSystemDirectoryHandle>
}
