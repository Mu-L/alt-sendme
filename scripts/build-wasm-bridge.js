#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const root = path.resolve(__dirname, '..')
const release = process.argv.includes('--release')
const profile = release ? 'release' : 'debug'
const cargoRelease = release ? ['--release'] : []

function run(cmd, args, opts = {}) {
	const result = spawnSync(cmd, args, {
		stdio: 'inherit',
		cwd: opts.cwd ?? root,
		env: opts.env ?? process.env,
		shell: opts.shell ?? false,
	})
	if (result.error) {
		throw result.error
	}
	if (result.status !== 0) {
		process.exit(result.status ?? 1)
	}
}

function resolveCc(env) {
	if (env.CC) {
		return env.CC
	}
	if (process.platform === 'darwin') {
		for (const candidate of [
			'/opt/homebrew/opt/llvm/bin/clang',
			'/usr/local/opt/llvm/bin/clang',
		]) {
			if (fs.existsSync(candidate)) {
				return candidate
			}
		}
	}
	if (process.platform === 'linux') {
		return 'clang'
	}
	if (process.platform === 'win32') {
		for (const candidate of [
			'C:\\Program Files\\LLVM\\bin\\clang.exe',
			'C:\\Program Files (x86)\\LLVM\\bin\\clang.exe',
		]) {
			if (fs.existsSync(candidate)) {
				return candidate
			}
		}
	}
	return undefined
}

function resolveWasmBindgen() {
	const lookup = spawnSync(
		process.platform === 'win32' ? 'where' : 'which',
		['wasm-bindgen'],
		{ encoding: 'utf8' }
	)
	if (lookup.status === 0) {
		const binary = lookup.stdout.trim().split(/\r?\n/)[0]
		if (binary) {
			return binary
		}
	}
	return null
}

const env = { ...process.env }
const cc = resolveCc(env)
if (cc) {
	env.CC = cc
}

const wasmBridgeDir = path.join(root, 'wasm-bridge')
env.CARGO_TARGET_DIR = path.join(root, 'wasm-bridge/target')

run(
	'cargo',
	['build', '--target', 'wasm32-unknown-unknown', ...cargoRelease],
	{ cwd: wasmBridgeDir, env }
)

const wasmPath = path.join(
	env.CARGO_TARGET_DIR,
	'wasm32-unknown-unknown',
	profile,
	'wasm_bridge.wasm'
)

let wasmBindgen = resolveWasmBindgen()
if (!wasmBindgen) {
	run('cargo', ['install', 'wasm-bindgen-cli', '--version', '0.2.126', '--locked'], {
		env,
	})
	wasmBindgen = resolveWasmBindgen()
}
if (!wasmBindgen) {
	console.error('build-wasm-bridge: wasm-bindgen not found after install')
	process.exit(1)
}

const outDir = path.join(root, 'frontend/src/wasm/pkg')
fs.mkdirSync(outDir, { recursive: true })

run(
	wasmBindgen,
	[
		wasmPath,
		'--target',
		'web',
		'--out-dir',
		outDir,
		'--out-name',
		'wasm_bridge',
		'--typescript',
	],
	{ env }
)

console.log(`WASM package written to ${outDir}`)
