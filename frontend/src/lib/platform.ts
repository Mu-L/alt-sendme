// VITE_APP_PLATFORM selects the app target (tauri vs web).
// TAURI_PLATFORM is injected by Vite define from TAURI_ENV_PLATFORM (OS target when on Tauri).
const appPlatform = import.meta.env.VITE_APP_PLATFORM ?? ''
const platform = import.meta.env.TAURI_PLATFORM ?? ''

export const IS_WEB = appPlatform === 'web'
export const IS_TAURI = appPlatform === 'tauri'
export const IS_ANDROID = IS_TAURI && platform.includes('android')
export const IS_MACOS = IS_TAURI && platform.includes('darwin')
export const IS_WINDOWS = IS_TAURI && platform.includes('windows')
export const IS_LINUX = IS_TAURI && platform.includes('linux')
