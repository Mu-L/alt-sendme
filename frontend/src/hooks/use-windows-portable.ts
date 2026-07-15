import { useQuery } from '@tanstack/react-query'
import { invoke } from '@/lib/platform-api'
import { IS_TAURI, IS_WEB, IS_WINDOWS } from '@/lib/platform'

export const portableQueryKeys = {
	windowsPortable: () => ['windows-portable'] as const,
}

/**
 * True when this session is the Windows no-install ZIP (`.portable` marker).
 * Cached for the app lifetime — the marker cannot appear/disappear mid-run.
 */
export function useIsWindowsPortable() {
	return useQuery({
		queryKey: portableQueryKeys.windowsPortable(),
		queryFn: async () => {
			if (IS_WEB || !IS_TAURI || !IS_WINDOWS) {
				return false
			}
			return invoke<boolean>('is_windows_portable')
		},
		staleTime: Number.POSITIVE_INFINITY,
		gcTime: Number.POSITIVE_INFINITY,
		retry: false,
	})
}

/** Helper for updater paths that are not React components. */
export async function isWindowsPortableBuild(): Promise<boolean> {
	if (IS_WEB || !IS_TAURI || !IS_WINDOWS) {
		return false
	}
	try {
		return await invoke<boolean>('is_windows_portable')
	} catch (error) {
		console.warn('Failed to detect Windows portable build:', error)
		return false
	}
}
