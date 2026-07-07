import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import {
	clearWebFiles,
	registerWebDirectory,
	registerWebFile,
} from './web-file-store.js'
import { collectWebSendPayload, webSendEntryType } from './web-send-items.js'

function makeFile(
	name: string,
	size: number,
	type = 'application/octet-stream'
): File {
	return new File([new Uint8Array(size)], name, { type })
}

describe('webSendEntryType', () => {
	it('detects directory, file, and collection selections', () => {
		clearWebFiles()
		registerWebDirectory('album')
		registerWebFile('notes.txt', makeFile('notes.txt', 1))

		assert.equal(webSendEntryType(['album']), 'directory')
		assert.equal(webSendEntryType(['notes.txt']), 'file')
		assert.equal(webSendEntryType(['notes.txt', 'album']), 'collection')
	})
})

describe('collectWebSendPayload', () => {
	it('collects all files under a selected folder', async () => {
		clearWebFiles()
		registerWebDirectory('album')
		registerWebFile('album/cover.jpg', makeFile('cover.jpg', 3))
		registerWebFile('album/readme.txt', makeFile('readme.txt', 2))

		const payload = await collectWebSendPayload(['album'])

		assert.equal(payload.entryType, 'directory')
		assert.deepEqual(payload.names, ['album/cover.jpg', 'album/readme.txt'])
		assert.equal(payload.bytesList.length, 2)
		assert.equal(payload.bytesList[0]?.byteLength, 3)
		assert.equal(payload.bytesList[1]?.byteLength, 2)
	})
})
