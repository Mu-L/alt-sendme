import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'node:path'

const host = process.env.TAURI_DEV_HOST

export default defineConfig(({ mode }) => {
	const isWeb = mode === 'web'

	return {
		plugins: [react()],
		base: isWeb ? '/web/' : '/',
		root: path.resolve(__dirname, './frontend'),
		resolve: {
			alias: {
				'@': path.resolve(__dirname, './frontend/src'),
				'lottie-web': 'lottie-web/build/player/lottie_light',
			},
		},
		define: {
			// Single string so platform.ts can derive flags; Vite replaces import.meta.env reliably
			'import.meta.env.TAURI_PLATFORM': JSON.stringify(
				process.env.TAURI_ENV_PLATFORM ?? ''
			),
		},
		// 1. prevent vite from obscuring rust errors
		clearScreen: false,
		// 2. tauri expects a fixed port, fail if that port is not available
		server: {
			port: isWeb ? 3001 : 1420,
			strictPort: true,
			host: host || false,
			hmr: host
				? {
						protocol: 'ws',
						host,
						port: isWeb ? 3002 : 1421,
					}
				: undefined,
			watch: {
				ignored: ['**/src-tauri/**'],
				usePolling: true,
			},
		},
	}
})
