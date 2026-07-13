import { useEffect, type Dispatch, type SetStateAction } from 'react'
import { listen } from '@/lib/platform-api'
import { IS_DESKTOP } from '@/lib/platform'
import {
	patchDevicePresence,
	type DevicePresencePayload,
	type DeviceUnpairedPayload,
	type PairedDevice,
} from '@/lib/pairing-api'

export function usePairedDeviceEvents(options: {
	onPresence: (payload: DevicePresencePayload) => void
	onUnpaired: (payload: DeviceUnpairedPayload) => void
	onRefresh: () => void
}) {
	const { onPresence, onUnpaired, onRefresh } = options

	useEffect(() => {
		if (!IS_DESKTOP) return

		let disposed = false
		let unlistenPresence: (() => void) | undefined
		let unlistenUnpaired: (() => void) | undefined
		let unlistenIdentityRotated: (() => void) | undefined

		const setup = async () => {
			const presenceUnlisten = await listen('paired-device-presence', (event) => {
				try {
					const payload =
						typeof event.payload === 'string'
							? (JSON.parse(event.payload) as DevicePresencePayload)
							: (event.payload as DevicePresencePayload)
					onPresence(payload)
				} catch (error) {
					console.error('Failed to parse paired-device-presence event:', error)
				}
			})
			if (disposed) {
				presenceUnlisten()
			} else {
				unlistenPresence = presenceUnlisten
			}

			const unpairedUnlisten = await listen('device-unpaired', (event) => {
				try {
					const payload =
						typeof event.payload === 'string'
							? (JSON.parse(event.payload) as DeviceUnpairedPayload)
							: (event.payload as DeviceUnpairedPayload)
					onUnpaired(payload)
					onRefresh()
				} catch (error) {
					console.error('Failed to parse device-unpaired event:', error)
					onRefresh()
				}
			})
			if (disposed) {
				unpairedUnlisten()
			} else {
				unlistenUnpaired = unpairedUnlisten
			}

			const identityRotatedUnlisten = await listen('identity-rotated', () => {
				onRefresh()
			})
			if (disposed) {
				identityRotatedUnlisten()
			} else {
				unlistenIdentityRotated = identityRotatedUnlisten
			}
		}

		void setup()

		return () => {
			disposed = true
			unlistenPresence?.()
			unlistenUnpaired?.()
			unlistenIdentityRotated?.()
		}
	}, [onPresence, onUnpaired, onRefresh])
}

export function applyPresencePatch(
	setDevices: Dispatch<SetStateAction<PairedDevice[]>>,
	payload: DevicePresencePayload
) {
	setDevices((prev) => patchDevicePresence(prev, payload))
}
