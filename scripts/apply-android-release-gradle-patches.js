#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const rootDir = path.resolve(__dirname, '..')
const buildGradle = path.join(
	rootDir,
	'src-tauri/gen/android/app/build.gradle.kts'
)
const keystoreProps = path.join(
	rootDir,
	'src-tauri/gen/android/keystore.properties'
)
const patchDir = path.join(__dirname, 'patches')

function gitApply(patchFile) {
	const rel = path.relative(rootDir, patchFile)
	const r = spawnSync('git', ['apply', rel], {
		cwd: rootDir,
		stdio: 'inherit',
	})
	return r.status === 0
}

if (!fs.existsSync(buildGradle)) {
	console.error('apply-android-release-gradle-patches: missing', buildGradle)
	console.error('Run `npx tauri android build --apk` (or android init) first.')
	process.exit(1)
}

let content = fs.readFileSync(buildGradle, 'utf8')

if (!content.includes('dependenciesInfo')) {
	const p = path.join(patchDir, '01-fdroid-gradle.patch')
	if (!gitApply(p)) {
		console.error(
			'apply-android-release-gradle-patches: 01-fdroid-gradle.patch failed.',
			'Regenerate against scripts/patches/android-app-build.gradle.kts.vanilla if Tauri updated the template.'
		)
		process.exit(1)
	}
	console.log('apply-android-release-gradle-patches: applied', path.basename(p))
} else {
	console.log(
		'apply-android-release-gradle-patches: skip 01 (dependenciesInfo already present)'
	)
}

content = fs.readFileSync(buildGradle, 'utf8')
const needsSigningPatch =
	fs.existsSync(keystoreProps) &&
	!(content.includes('signingConfigs') && content.includes('create("release")'))

if (needsSigningPatch) {
	const p = path.join(patchDir, '02-signing-gradle.patch')
	if (!gitApply(p)) {
		console.error(
			'apply-android-release-gradle-patches: 02-signing-gradle.patch failed.'
		)
		process.exit(1)
	}
	console.log('apply-android-release-gradle-patches: applied', path.basename(p))
} else if (fs.existsSync(keystoreProps)) {
	console.log(
		'apply-android-release-gradle-patches: skip 02 (release signing already present)'
	)
}
