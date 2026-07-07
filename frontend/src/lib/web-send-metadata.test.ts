import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import {
	clearWebFiles,
	registerWebDirectory,
	registerWebFile,
} from './web-file-store.js'
import {
	buildWebSendMetadataForFile,
	buildWebSendMetadataForPaths,
} from './web-send-metadata.js'

function makeFile(
	name: string,
	size: number,
	type = 'application/octet-stream'
): File {
	return new File([new Uint8Array(size)], name, { type })
}

describe('buildWebSendMetadataForFile', () => {
	it('sets thumbnail to null and includes mime type', () => {
		const file = makeFile('photo.png', 42, 'image/png')
		const metadata = JSON.parse(buildWebSendMetadataForFile(file))

		assert.equal(metadata.file_name, 'photo.png')
		assert.equal(metadata.item_count, 1)
		assert.equal(metadata.size, 42)
		assert.equal(metadata.mime_type, 'image/png')
		assert.equal(metadata.thumbnail, null)
		assert.equal(metadata.items, undefined)
	})
})

describe('buildWebSendMetadataForPaths', () => {
	it('builds single-file metadata without thumbnails', () => {
		clearWebFiles()
		const file = makeFile('notes.txt', 10, 'text/plain')
		registerWebFile('notes.txt', file)

		const metadata = JSON.parse(buildWebSendMetadataForPaths(['notes.txt']))

		assert.equal(metadata.item_count, 1)
		assert.equal(metadata.thumbnail, null)
		assert.equal(metadata.mime_type, 'text/plain')
	})

	it('builds directory metadata with inode/directory mime type', () => {
		clearWebFiles()
		registerWebDirectory('album')
		registerWebFile('album/cover.jpg', makeFile('cover.jpg', 100, 'image/jpeg'))
		registerWebFile(
			'album/readme.txt',
			makeFile('readme.txt', 20, 'text/plain')
		)

		const metadata = JSON.parse(buildWebSendMetadataForPaths(['album']))

		assert.equal(metadata.file_name, 'album')
		assert.equal(metadata.item_count, 1)
		assert.equal(metadata.size, 120)
		assert.equal(metadata.mime_type, 'inode/directory')
		assert.equal(metadata.thumbnail, null)
		assert.equal(metadata.items, undefined)
	})

	it('builds collection metadata with per-item thumbnails set to null', () => {
		clearWebFiles()
		registerWebFile('a.png', makeFile('a.png', 5, 'image/png'))
		registerWebFile('b.pdf', makeFile('b.pdf', 7, 'application/pdf'))

		const metadata = JSON.parse(
			buildWebSendMetadataForPaths(['a.png', 'b.pdf'])
		)

		assert.equal(metadata.item_count, 2)
		assert.equal(metadata.mime_type, 'application/x-iroh-collection')
		assert.equal(metadata.thumbnail, null)
		assert.deepEqual(metadata.items, [
			{
				file_name: 'a.png',
				size: 5,
				mime_type: 'image/png',
				thumbnail: null,
			},
			{
				file_name: 'b.pdf',
				size: 7,
				mime_type: 'application/pdf',
				thumbnail: null,
			},
		])
	})
})
