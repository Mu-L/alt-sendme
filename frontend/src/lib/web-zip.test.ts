import assert from 'node:assert/strict'
import { describe, it } from 'node:test'
import { createStoreOnlyZip } from './web-zip.js'

describe('createStoreOnlyZip', () => {
	it('creates a readable store-only archive for nested files', () => {
		const zip = createStoreOnlyZip([
			{
				path: 'album/cover.jpg',
				bytes: new Uint8Array([1, 2, 3]),
			},
			{
				path: 'album/readme.txt',
				bytes: new Uint8Array([4, 5]),
			},
		])

		assert.ok(zip.byteLength > 0)
		assert.ok(
			new TextDecoder().decode(zip).includes('album/cover.jpg'),
			'zip should contain entry path'
		)
		assert.ok(
			new TextDecoder().decode(zip).includes('album/readme.txt'),
			'zip should contain second entry path'
		)
	})
})
