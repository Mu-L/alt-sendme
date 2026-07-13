import { invoke } from './platform-api'
import { IS_DESKTOP } from './platform'
import type { RelayConfigArg } from './relay-config'

export interface DeviceInfo {
	endpoint_id: string
	display_name: string
	device_type: string
	os: string
}

export type PairingStatus = 'active' | 'unpaired-remotely' | 'stale-local-identity'

export interface PairedDevice {
	endpoint_id: string
	display_name: string
	device_type: string
	os: string
	paired_at: number
	last_seen_at: number
	pairing_status: PairingStatus
	online: boolean
}

export interface DevicePresencePayload {
	endpoint_id: string
	online: boolean
	last_seen_at: number
}

export interface DeviceUnpairedPayload {
	endpoint_id: string
	display_name?: string
	reason: 'local' | 'remote'
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

export async function setDeviceDisplayName(
	displayName: string
): Promise<DeviceInfo | null> {
	if (!desktopOnly()) return null
	return invoke<DeviceInfo>('set_device_display_name', {
		displayName,
	})
}

export async function startPairingHost(options?: {
	ttlSecs?: number | null
}): Promise<string> {
	if (!desktopOnly()) {
		throw new Error('Device pairing is only available on desktop')
	}
	const ttlSecs = options?.ttlSecs ?? null
	return invoke<string>('start_pairing_host', { ttlSecs })
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

export async function renamePairedDevice(
	endpointId: string,
	displayName: string
): Promise<PairedDevice | null> {
	if (!desktopOnly()) return null
	return invoke<PairedDevice>('rename_paired_device', {
		endpointId,
		displayName,
	})
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

export function formatOsLabel(os: string | undefined | null): string {
	switch ((os ?? '').toLowerCase()) {
		case 'macos':
			return 'macOS'
		case 'windows':
			return 'Windows'
		case 'linux':
			return 'Linux'
		case 'ios':
			return 'iOS'
		case 'android':
			return 'Android'
		default:
			return os?.trim() || ''
	}
}

export function formatDeviceTypeLabel(
	deviceType: string | undefined | null
): string {
	switch ((deviceType ?? '').toLowerCase()) {
		case 'laptop':
			return 'Laptop'
		case 'desktop':
			return 'Desktop'
		case 'phone':
			return 'Phone'
		case 'tablet':
			return 'Tablet'
		default:
			return deviceType?.trim() || 'Device'
	}
}

export function deviceSubtitle(
	device: Pick<PairedDevice, 'device_type' | 'os'> | Pick<DeviceInfo, 'device_type' | 'os'>
): string {
	const typeLabel = formatDeviceTypeLabel(device.device_type)
	const osLabel = formatOsLabel(device.os)
	return osLabel ? `${typeLabel} ${osLabel}` : typeLabel
}

export function matchesPairedDeviceSearch(
	device: PairedDevice,
	query: string
): boolean {
	const normalizedQuery = query.trim().toLowerCase()
	if (!normalizedQuery) return true

	const searchable = [
		device.display_name,
		device.device_type,
		device.os,
		formatDeviceTypeLabel(device.device_type),
		formatOsLabel(device.os),
		deviceSubtitle(device),
		device.online ? 'online' : 'offline',
		isPairedDeviceActive(device) ? 'active' : 'unpaired',
	]
		.join(' ')
		.toLowerCase()

	return searchable.includes(normalizedQuery)
}

export function isPairedDeviceActive(
	device: Pick<PairedDevice, 'pairing_status'>
): boolean {
	return (device.pairing_status ?? 'active') === 'active'
}

export function patchDevicePresence(
	devices: PairedDevice[],
	payload: DevicePresencePayload
): PairedDevice[] {
	const id = payload.endpoint_id.toLowerCase()
	return devices.map((device) =>
		device.endpoint_id.toLowerCase() === id
			? {
					...device,
					online: payload.online,
					last_seen_at: payload.last_seen_at,
				}
			: device
	)
}
