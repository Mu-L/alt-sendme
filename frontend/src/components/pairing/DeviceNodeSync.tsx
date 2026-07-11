import { useEffect } from 'react'
import { listen } from '@/lib/platform-api'
import { IS_DESKTOP } from '@/lib/platform'
import { getRelayConfigArg } from '@/lib/relay'
import { reconfigureNodeRelay } from '@/lib/pairing-api'
import type { PairedInvitePayload } from '@/lib/pairing-api'
import { usePairedInviteStore } from '@/store/paired-invite-store'
import { useNodeCapability } from '@/hooks/useNodeCapability'

/** Syncs relay settings to the device node and listens for paired invites globally. */
export function DeviceNodeSync() {
	const { isNodeReady, refreshNodeStatus } = useNodeCapability()
	const setInvite = usePairedInviteStore((s) => s.setInvite)

	useEffect(() => {
		if (!IS_DESKTOP || !isNodeReady) return
		void reconfigureNodeRelay(getRelayConfigArg()).catch((error) => {
			console.warn('Failed to sync node relay on startup:', error)
		})
	}, [isNodeReady])

	useEffect(() => {
		if (!IS_DESKTOP) return

		let disposed = false
		let unlistenInvite: (() => void) | undefined
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
			unlistenExpired?.()
		}
	}, [setInvite, refreshNodeStatus])

	return null
}
