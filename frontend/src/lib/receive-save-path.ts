import { IS_ANDROID } from '@/lib/platform'

export function formatReceiveSavePath(path: string | undefined | null): string {
	if (!path) return ''

	if (!IS_ANDROID) return path

	const normalized = path.replace(/\\/g, '/')
	const segments = normalized.split('/').filter(Boolean)

	if (segments.length <= 2) return segments.join('/')

	return segments.slice(-2).join('/')
}
