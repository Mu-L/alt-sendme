import { Link } from 'react-router-dom'
import { useEffect, useMemo, useState } from 'react'
import { Loader2 } from 'lucide-react'
import { useTranslation } from '../../i18n/react-i18next-compat'
import {
	deviceSubtitle,
	isPairedDeviceActive,
	matchesPairedDeviceSearch,
	sortPairedDevicesForList,
	type PairedDevice,
} from '@/lib/pairing-api'
import { getPairedSendCounts } from '@/lib/paired-send-counts'
import { deviceTypeIcon } from '@/lib/device-icon'
import { cn } from '@/lib/utils'
import { DevicePairingStatus } from '../pairing/DevicePairingStatus'
import { PairedDevicesSearchField } from '../pairing/PairedDevicesSearchField'

type PairedInviteStatus = 'sending' | 'sent' | 'failed'

interface PairedDevicesPanelProps {
	pairedDevices: PairedDevice[]
	pairedInviteStatus: Record<string, PairedInviteStatus>
	isNodeReady: boolean
	hasTicket: boolean
	onInvitePairedDevice?: (endpointId: string) => Promise<boolean>
	onInviteSuccess?: () => void
	showHeader?: boolean
	showSearch?: boolean
	isOpen?: boolean
}

export function PairedDevicesPanel({
	pairedDevices,
	pairedInviteStatus,
	isNodeReady,
	hasTicket,
	onInvitePairedDevice,
	onInviteSuccess,
	showHeader = true,
	showSearch = false,
	isOpen = false,
}: PairedDevicesPanelProps) {
	const { t } = useTranslation()
	const [searchQuery, setSearchQuery] = useState('')

	useEffect(() => {
		if (isOpen) {
			setSearchQuery('')
		}
	}, [isOpen])

	const sortedDevices = useMemo(
		() => sortPairedDevicesForList(pairedDevices, getPairedSendCounts()),
		[pairedDevices]
	)

	const filteredDevices = useMemo(() => {
		if (!showSearch) return sortedDevices
		return sortedDevices.filter((device) =>
			matchesPairedDeviceSearch(device, searchQuery)
		)
	}, [sortedDevices, searchQuery, showSearch])

	const handleSelectDevice = async (endpointId: string) => {
		const success = await onInvitePairedDevice?.(endpointId)
		if (success) {
			onInviteSuccess?.()
		}
	}

	const listContent =
		sortedDevices.length === 0 ? (
			<div className="space-y-2 rounded-md border border-dashed px-3 py-6 text-center text-xs text-muted-foreground">
				<p className="font-medium text-foreground">
					{t('common:sender.sharingActive.devices.emptyTitle')}
				</p>
				<p>{t('common:sender.sharingActive.devices.emptyHint')}</p>
				<Link
					to="/settings/devices"
					className="inline-block underline underline-offset-2 hover:text-foreground"
				>
					{t('common:sender.sharingActive.devices.manageInSettings')}
				</Link>
			</div>
		) : filteredDevices.length === 0 ? (
			<p className="rounded-md border border-dashed px-3 py-6 text-center text-xs text-muted-foreground">
				{t('common:sender.sharingActive.devices.searchNoResults')}
			</p>
		) : (
			<ul className="divide-y divide-border">
				{filteredDevices.map((device) => {
					const Icon = deviceTypeIcon(device.device_type)
					const inviteStatus = pairedInviteStatus[device.endpoint_id]
					const isActive = isPairedDeviceActive(device)
					const isSending = inviteStatus === 'sending'
					const anotherDeviceSelected = Object.entries(
						pairedInviteStatus
					).some(
						([id, status]) =>
							id !== device.endpoint_id &&
							(status === 'sending' || status === 'sent')
					)
					const disabled =
						!isNodeReady ||
						!hasTicket ||
						isSending ||
						!isActive ||
						anotherDeviceSelected ||
						inviteStatus === 'sent'
					return (
						<li key={device.endpoint_id}>
							<button
								type="button"
								disabled={disabled}
								onClick={() => handleSelectDevice(device.endpoint_id)}
								aria-label={t('common:sender.pairedDevices.send')}
								className={cn(
									'flex w-full items-center justify-between gap-3 rounded-md px-3 py-3 text-left text-sm transition-colors',
									'hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring',
									'disabled:pointer-events-none disabled:opacity-50'
								)}
							>
								<div className="flex min-w-0 items-center gap-3">
									<div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
										{isSending ? (
											<Loader2 className="h-4 w-4 animate-spin" />
										) : (
											<Icon className="h-4 w-4" />
										)}
									</div>
									<div className="min-w-0">
										<span className="block truncate">
											{device.display_name}
										</span>
										<span className="block truncate text-xs text-muted-foreground">
											{isSending
												? t('common:sender.pairedDevices.sending')
												: deviceSubtitle(device)}
										</span>
									</div>
								</div>
								{!isSending ? (
									<DevicePairingStatus
										device={device}
										namespace="sender"
										className="shrink-0"
									/>
								) : null}
							</button>
						</li>
					)
				})}
			</ul>
		)

	return (
		<div
			className={cn(
				showSearch
					? 'flex h-full min-h-0 flex-col gap-2'
					: 'space-y-4'
			)}
		>
			{showHeader ? (
				<div className="space-y-0.5">
					<p className="text-sm font-medium">
						{t('common:sender.sharingActive.devices.title')}
					</p>
					<p className="text-xs text-muted-foreground">
						{t('common:sender.sharingActive.devices.hint')}
					</p>
				</div>
			) : null}

			{!isNodeReady && (
				<p className="shrink-0 rounded-md border border-dashed px-3 py-3 text-xs text-muted-foreground">
					{t('common:sender.sharingActive.devices.nodeUnavailable')}
				</p>
			)}

			{showSearch && sortedDevices.length > 0 ? (
				<PairedDevicesSearchField
					value={searchQuery}
					onChange={setSearchQuery}
					className="shrink-0"
				/>
			) : null}

			{showSearch ? (
				<div className="min-h-0 flex-1 overflow-y-auto">{listContent}</div>
			) : (
				listContent
			)}
		</div>
	)
}
