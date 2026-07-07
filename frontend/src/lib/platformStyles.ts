import { IS_LINUX, IS_MACOS, IS_TAURI, IS_WEB, IS_WINDOWS } from './platform'

/** Match default Tauri window size in `src-tauri/tauri.conf.json`. */
export const WEB_APP_MAX_WIDTH = 1024
export const WEB_APP_MAX_HEIGHT = 680
export const WEB_APP_PORTAL_ID = 'web-app-portal'

export function getWebAppOverlayContainer(): HTMLElement | undefined {
	if (!IS_WEB) {
		return undefined
	}
	return document.getElementById(WEB_APP_PORTAL_ID) ?? undefined
}

export function getPlatformAlpha(): number {
	if (!IS_TAURI) return 1

	if (IS_MACOS) return 0.4

	if (IS_WINDOWS || IS_LINUX) return 1

	return 1
}

export function initializePlatformStyles(): void {
	const root = document.documentElement

	if (IS_WEB) {
		document.body.classList.add('web-app-shell')
		root.style.setProperty('--web-app-max-width', `${WEB_APP_MAX_WIDTH}px`)
		root.style.setProperty('--web-app-max-height', `${WEB_APP_MAX_HEIGHT}px`)
	}

	if (IS_TAURI && IS_MACOS) {
		root.style.setProperty('--body-bg', 'transparent')
	}
}
