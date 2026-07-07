export type ZipEntry = {
	path: string
	bytes: Uint8Array
}

const CRC_TABLE = (() => {
	const table = new Uint32Array(256)
	for (let index = 0; index < 256; index++) {
		let value = index
		for (let bit = 0; bit < 8; bit++) {
			value = value & 1 ? 0xedb88320 ^ (value >>> 1) : value >>> 1
		}
		table[index] = value >>> 0
	}
	return table
})()

function crc32(bytes: Uint8Array): number {
	let crc = 0xffffffff
	for (const byte of bytes) {
		crc = (CRC_TABLE[(crc ^ byte) & 0xff]! ^ (crc >>> 8)) >>> 0
	}
	return (crc ^ 0xffffffff) >>> 0
}

function writeUint16LE(view: DataView, offset: number, value: number) {
	view.setUint16(offset, value, true)
}

function writeUint32LE(view: DataView, offset: number, value: number) {
	view.setUint32(offset, value, true)
}

function concatChunks(chunks: Uint8Array[]): Uint8Array {
	const total = chunks.reduce((sum, chunk) => sum + chunk.byteLength, 0)
	const output = new Uint8Array(total)
	let offset = 0
	for (const chunk of chunks) {
		output.set(chunk, offset)
		offset += chunk.byteLength
	}
	return output
}

function buildLocalFileHeader(
	nameBytes: Uint8Array,
	crc: number,
	size: number
): Uint8Array {
	const header = new Uint8Array(30 + nameBytes.length)
	const view = new DataView(header.buffer)
	writeUint32LE(view, 0, 0x04034b50)
	writeUint16LE(view, 4, 20)
	writeUint16LE(view, 6, 0)
	writeUint16LE(view, 8, 0)
	writeUint16LE(view, 10, 0)
	writeUint16LE(view, 12, 0)
	writeUint32LE(view, 14, crc)
	writeUint32LE(view, 18, size)
	writeUint32LE(view, 22, size)
	writeUint16LE(view, 26, nameBytes.length)
	writeUint16LE(view, 28, 0)
	header.set(nameBytes, 30)
	return header
}

function buildCentralDirectoryHeader(
	nameBytes: Uint8Array,
	crc: number,
	size: number,
	offset: number
): Uint8Array {
	const header = new Uint8Array(46 + nameBytes.length)
	const view = new DataView(header.buffer)
	writeUint32LE(view, 0, 0x02014b50)
	writeUint16LE(view, 4, 20)
	writeUint16LE(view, 6, 20)
	writeUint16LE(view, 8, 0)
	writeUint16LE(view, 10, 0)
	writeUint16LE(view, 12, 0)
	writeUint16LE(view, 14, 0)
	writeUint32LE(view, 16, crc)
	writeUint32LE(view, 20, size)
	writeUint32LE(view, 24, size)
	writeUint16LE(view, 28, nameBytes.length)
	writeUint16LE(view, 30, 0)
	writeUint16LE(view, 32, 0)
	writeUint16LE(view, 34, 0)
	writeUint16LE(view, 36, 0)
	writeUint32LE(view, 38, 0)
	writeUint32LE(view, 42, offset)
	header.set(nameBytes, 46)
	return header
}

function buildEndOfCentralDirectory(
	entryCount: number,
	centralSize: number,
	centralOffset: number
): Uint8Array {
	const header = new Uint8Array(22)
	const view = new DataView(header.buffer)
	writeUint32LE(view, 0, 0x06054b50)
	writeUint16LE(view, 4, 0)
	writeUint16LE(view, 6, 0)
	writeUint16LE(view, 8, entryCount)
	writeUint16LE(view, 10, entryCount)
	writeUint32LE(view, 12, centralSize)
	writeUint32LE(view, 16, centralOffset)
	writeUint16LE(view, 20, 0)
	return header
}

/** Create a store-only ZIP archive for browser download fallback. */
export function createStoreOnlyZip(entries: ZipEntry[]): Uint8Array {
	if (!entries.length) {
		throw new Error('ZIP archive requires at least one file')
	}

	const localChunks: Uint8Array[] = []
	const centralChunks: Uint8Array[] = []
	let offset = 0

	for (const entry of entries) {
		const normalizedPath = entry.path.replace(/\\/g, '/')
		const nameBytes = new TextEncoder().encode(normalizedPath)
		const checksum = crc32(entry.bytes)
		const size = entry.bytes.byteLength
		const localHeader = buildLocalFileHeader(nameBytes, checksum, size)

		localChunks.push(localHeader, entry.bytes)
		centralChunks.push(
			buildCentralDirectoryHeader(nameBytes, checksum, size, offset)
		)
		offset += localHeader.byteLength + size
	}

	const centralDirectory = concatChunks(centralChunks)
	const endRecord = buildEndOfCentralDirectory(
		entries.length,
		centralDirectory.byteLength,
		offset
	)

	return concatChunks([...localChunks, centralDirectory, endRecord])
}
