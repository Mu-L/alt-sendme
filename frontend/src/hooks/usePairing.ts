import { useCallback, useEffect, useRef, useState } from 'react'
import { listen } from '@/lib/platform-api'
import { IS_DESKTOP } from '@/lib/platform'
import {
	forgetPairedDevice,
	getDeviceInfo,
	joinPairing,
	listPairedDevices,
	renamePairedDevice,
	setDeviceDisplayName,
	startPairingHost,
	stopPairingHost,
	type DeviceInfo,
	type PairedDevice,
} from '@/lib/pairing-api'
import { useNodeCapability } from './useNodeCapability'

// Must match engine/protocol pairing::PAIRING_VOTE_TIMEOUT_SECS
const PAIRING_HOST_TTL_SECS = 120

export function usePairing() {
	const [devices, setDevices] = useState<PairedDevice[]>([])
	const [thisDevice, setThisDevice] = useState<DeviceInfo | null>(null)
	const [pairingTicket, setPairingTicket] = useState<string | null>(null)
	const [hostExpiresIn, setHostExpiresIn] = useState<number | null>(null)
	const [isJoining, setIsJoining] = useState(false)
	const [isLoading, setIsLoading] = useState(false)
	// Incremented each time a peer joins while this device is hosting a
	// pairing window, so the UI can close the QR dialog and confirm success.
	const [hostPairedCount, setHostPairedCount] = useState(0)
	const pairingTicketRef = useRef<string | null>(null)
	const { isNodeReady, nodeStatus } = useNodeCapability()

	useEffect(() => {
		pairingTicketRef.current = pairingTicket
	}, [pairingTicket])

	const refreshDevices = useCallback(async () => {
		if (!IS_DESKTOP || !isNodeReady) {
			setDevices([])
			return
		}
		try {
			setDevices(await listPairedDevices())
		} catch (error) {
			console.error('Failed to list paired devices:', error)
		}
	}, [isNodeReady])

	const refreshThisDevice = useCallback(async () => {
		if (!IS_DESKTOP || !isNodeReady) {
			setThisDevice(null)
			return
		}
		try {
			setThisDevice(await getDeviceInfo())
		} catch (error) {
			console.error('Failed to load this device:', error)
		}
	}, [isNodeReady])

	useEffect(() => {
		void refreshDevices()
		void refreshThisDevice()
	}, [refreshDevices, refreshThisDevice])

	useEffect(() => {
		if (!IS_DESKTOP) return

		let disposed = false
		let unlistenPaired: (() => void) | undefined
		let unlistenExpired: (() => void) | undefined

		const setup = async () => {
			const pairedUnlisten = await listen('device-paired', () => {
				// The backend closes the pairing host once a peer completes the
				// handshake, so the ticket is no longer valid.
				if (pairingTicketRef.current != null) {
					setPairingTicket(null)
					setHostExpiresIn(null)
					setHostPairedCount((count) => count + 1)
				}
				void refreshDevices()
			})
			if (disposed) {
				pairedUnlisten()
			} else {
				unlistenPaired = pairedUnlisten
			}

			const expiredUnlisten = await listen('pairing-host-expired', () => {
				setPairingTicket(null)
				setHostExpiresIn(null)
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
			unlistenPaired?.()
			unlistenExpired?.()
		}
	}, [refreshDevices])

	useEffect(() => {
		if (hostExpiresIn == null || hostExpiresIn <= 0) return

		const timer = window.setInterval(() => {
			setHostExpiresIn((prev) => {
				if (prev == null || prev <= 1) {
					window.clearInterval(timer)
					return null
				}
				return prev - 1
			})
		}, 1000)

		return () => window.clearInterval(timer)
	}, [hostExpiresIn])

	const openHostPairing = useCallback(async () => {
		if (!IS_DESKTOP || !isNodeReady) return null
		setIsLoading(true)
		try {
			const ticket = await startPairingHost({ ttlSecs: 120 })
			setPairingTicket(ticket)
			setHostExpiresIn(PAIRING_HOST_TTL_SECS)
			return ticket
		} finally {
			setIsLoading(false)
		}
	}, [isNodeReady])

	const closeHostPairing = useCallback(async () => {
		setPairingTicket(null)
		setHostExpiresIn(null)
		await stopPairingHost()
	}, [])

	const join = useCallback(
		async (ticket: string) => {
			if (!IS_DESKTOP || !isNodeReady) return
			setIsJoining(true)
			try {
				await joinPairing(ticket.trim())
				await refreshDevices()
			} finally {
				setIsJoining(false)
			}
		},
		[isNodeReady, refreshDevices]
	)

	const forget = useCallback(
		async (endpointId: string) => {
			await forgetPairedDevice(endpointId)
			await refreshDevices()
		},
		[refreshDevices]
	)

	const renameThisDevice = useCallback(
		async (displayName: string) => {
			const updated = await setDeviceDisplayName(displayName)
			if (updated) setThisDevice(updated)
			return updated
		},
		[]
	)

	const renameDevice = useCallback(
		async (endpointId: string, displayName: string) => {
			const updated = await renamePairedDevice(endpointId, displayName)
			await refreshDevices()
			return updated
		},
		[refreshDevices]
	)

	return {
		devices,
		thisDevice,
		pairingTicket,
		hostExpiresIn,
		isJoining,
		isLoading,
		hostPairedCount,
		isNodeReady,
		nodeStatus,
		refreshDevices,
		refreshThisDevice,
		openHostPairing,
		closeHostPairing,
		join,
		forget,
		renameThisDevice,
		renameDevice,
	}
}
