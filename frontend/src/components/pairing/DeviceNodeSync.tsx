import { useEffect, useRef } from 'react'
import { listen } from '@/lib/platform-api'
import { IS_DESKTOP } from '@/lib/platform'
import { getRelayConfigArg } from '@/lib/relay'
import { reconfigureNodeRelay } from '@/lib/pairing-api'
import type {
	PairedInvitePayload,
	PairedInviteResponsePayload,
} from '@/lib/pairing-api'
import { usePairedInviteStore } from '@/store/paired-invite-store'
import { preloadPairingData } from '@/store/pairing-data-store'
import { useNodeCapability } from '@/hooks/useNodeCapability'
import { useTranslation } from '@/i18n'
import { toastManager } from '../ui/toast'

/** Syncs relay settings to the device node and listens for paired invites globally. */
export function DeviceNodeSync() {
	const { t } = useTranslation()
	const { isNodeReady, refreshNodeStatus } = useNodeCapability()
	const setInvite = usePairedInviteStore((s) => s.setInvite)
	const didSyncRelay = useRef(false)

	// Warm node status + devices/this-device before settings opens, so the
	// first Devices visit paints complete content instead of loading → ready.
	useEffect(() => {
		if (!IS_DESKTOP) return
		void preloadPairingData()
	}, [])

	useEffect(() => {
		if (!IS_DESKTOP || !isNodeReady || didSyncRelay.current) return
		didSyncRelay.current = true
		void reconfigureNodeRelay(getRelayConfigArg()).catch((error) => {
			// Allow a later retry if the first sync failed (e.g. node still settling).
			didSyncRelay.current = false
			console.warn('Failed to sync node relay on startup:', error)
		})
	}, [isNodeReady])

	useEffect(() => {
		if (!IS_DESKTOP) return

		let disposed = false
		let unlistenInvite: (() => void) | undefined
		let unlistenResponse: (() => void) | undefined
		let unlistenExpired: (() => void) | undefined

		const setup = async () => {
			const inviteUnlisten = await listen(
				'paired-invite-received',
				(event: { payload: unknown }) => {
					console.log('[paired-invite] receiver: event received', {
						payloadType: typeof event.payload,
						payloadPreview: String(event.payload).slice(0, 120),
					})
					try {
						const payload = JSON.parse(
							String(event.payload)
						) as PairedInvitePayload
						console.log('[paired-invite] receiver: parsed invite', {
							sender: payload.sender_name,
							fileCount: payload.file_count,
							totalSize: payload.total_size,
							remoteEndpointId: payload.remote_endpoint_id,
							ticketLen: payload.blob_ticket.length,
						})
						setInvite(payload)
					} catch (error) {
						console.error('[paired-invite] receiver: parse failed', error)
					}
				}
			)
			if (disposed) {
				inviteUnlisten()
			} else {
				unlistenInvite = inviteUnlisten
			}

			const responseUnlisten = await listen(
				'paired-invite-response',
				(event: { payload: unknown }) => {
					try {
						const payload = JSON.parse(
							String(event.payload)
						) as PairedInviteResponsePayload
						if (payload.response !== 'declined') return
						const name =
							payload.display_name?.trim() ||
							t('common:sender.pairedDevices.unknownPeer')
						toastManager.add({
							title: t('common:sender.pairedDevices.inviteDeclined', {
								name,
							}),
							description: t(
								'common:sender.pairedDevices.inviteDeclinedDesc'
							),
							type: 'warning',
						})
					} catch (error) {
						console.error(
							'[paired-invite] sender: response parse failed',
							error
						)
					}
				}
			)
			if (disposed) {
				responseUnlisten()
			} else {
				unlistenResponse = responseUnlisten
			}

			const expiredUnlisten = await listen('pairing-host-expired', () => {
				void refreshNodeStatus()
			})
			if (disposed) {
				expiredUnlisten()
			} else {
				unlistenExpired = expiredUnlisten
			}
		}

		void setup()

		return () => {
			disposed = true
			unlistenInvite?.()
			unlistenResponse?.()
			unlistenExpired?.()
		}
	}, [setInvite, refreshNodeStatus, t])

	return null
}
