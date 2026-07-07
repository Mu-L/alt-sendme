import { relayAuthTokenForIpc } from './relay-auth-token.js'
import { IS_WEB } from './platform.js'

export type RelayMode = 'default' | 'custom' | 'disabled'
export type RelayFallback = 'strict' | 'public'

export type RelayConfigArg = {
	mode: RelayMode
	urls: string[]
	auth_token?: string | null
	fallback: RelayFallback
}

export type RelayConfigInput = {
	relayMode: RelayMode
	relayUrls: string[]
	relayAuthToken: string
	relayFallback: RelayFallback
}

/** Browser transfers require relays; desktop-only "disabled" mode is not offered on web. */
export function effectiveRelayMode(relayMode: RelayMode): RelayMode {
	return IS_WEB && relayMode === 'disabled' ? 'default' : relayMode
}

export function buildRelayConfigArg({
	relayMode,
	relayUrls,
	relayAuthToken,
	relayFallback,
}: RelayConfigInput): RelayConfigArg {
	const mode = effectiveRelayMode(relayMode)

	return {
		mode,
		urls:
			mode === 'custom'
				? relayUrls.map((url) => url.trim()).filter(Boolean)
				: [],
		auth_token: mode === 'custom' ? relayAuthTokenForIpc(relayAuthToken) : null,
		fallback: relayFallback,
	}
}
