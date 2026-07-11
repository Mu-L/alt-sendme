import { invoke } from './platform-api'
import { IS_DESKTOP } from './platform'
import type { RelayConfigArg } from './relay-config'

export interface DeviceInfo {
	endpoint_id: string
	display_name: string
	device_type: string
}

export interface PairedDevice {
	endpoint_id: string
	display_name: string
	device_type: string
	paired_at: number
	last_seen_at: number
}

export interface PairedInvitePayload {
	blob_ticket: string
	file_count: number
	total_size: number
	sender_name: string
	remote_endpoint_id: string
}

export interface InviteDelivered {
	delivered: boolean
}

export type NodeStatusKind = 'ready' | 'unavailable'

export interface NodeStatus {
	status: NodeStatusKind
	reason?: string | null
}

function desktopOnly(): boolean {
	return IS_DESKTOP
}

export async function getNodeStatus(): Promise<NodeStatus> {
	if (!desktopOnly()) {
		return { status: 'unavailable', reason: 'desktop_only' }
	}
	return invoke<NodeStatus>('get_node_status')
}

export async function reconfigureNodeRelay(
	relay: RelayConfigArg
): Promise<void> {
	if (!desktopOnly()) return
	await invoke('reconfigure_node_relay', { relay })
}

export async function getDeviceInfo(): Promise<DeviceInfo | null> {
	if (!desktopOnly()) return null
	return invoke<DeviceInfo>('get_device_info')
}

export async function startPairingHost(): Promise<string> {
	if (!desktopOnly()) {
		throw new Error('Device pairing is only available on desktop')
	}
	return invoke<string>('start_pairing_host')
}

export async function stopPairingHost(): Promise<void> {
	if (!desktopOnly()) return
	await invoke('stop_pairing_host')
}

export async function joinPairing(ticket: string): Promise<void> {
	if (!desktopOnly()) {
		throw new Error('Device pairing is only available on desktop')
	}
	await invoke('join_pairing', { ticket })
}

export async function listPairedDevices(): Promise<PairedDevice[]> {
	if (!desktopOnly()) return []
	return invoke<PairedDevice[]>('list_paired_devices')
}

export async function forgetPairedDevice(endpointId: string): Promise<void> {
	if (!desktopOnly()) return
	await invoke('forget_paired_device', { endpointId })
}

export async function invitePairedDevice(
	endpointId: string,
	blobTicket: string,
	fileCount: number,
	totalSize: number
): Promise<boolean> {
	if (!desktopOnly()) return false
	console.log('[paired-invite] sender: invoking invite_paired_device', {
		endpointId,
		fileCount,
		totalSize,
		ticketLen: blobTicket.length,
	})
	const result = await invoke<InviteDelivered>('invite_paired_device', {
		endpointId,
		blobTicket,
		fileCount,
		totalSize,
	})
	console.log('[paired-invite] sender: invite_paired_device returned', result)
	return result.delivered
}
