import { useState } from 'react'
import { Link } from 'react-router-dom'
import { Check, ChevronDown, Copy, CheckCircle, Loader2 } from 'lucide-react'
import { useTranslation } from '../../i18n/react-i18next-compat'
import type { PairedDevice } from '@/lib/pairing-api'
import { deviceSubtitle } from '@/lib/pairing-api'
import { deviceTypeIcon } from '@/lib/device-icon'
import { Button } from '../ui/button'
import { InputGroup, InputGroupAddon, InputGroupInput } from '../ui/input-group'
import {
	Collapsible,
	CollapsiblePanel,
	CollapsibleTrigger,
} from '../ui/collapsible'
import { cn } from '@/lib/utils'

type PairedInviteStatus = 'sending' | 'sent' | 'failed'

interface PairedDevicesPanelProps {
	pairedDevices: PairedDevice[]
	pairedInviteStatus: Record<string, PairedInviteStatus>
	isNodeReady: boolean
	hasTicket: boolean
	pairingTicket: string | null
	pairingCopySuccess: boolean
	onInvitePairedDevice?: (endpointId: string) => Promise<void>
	onCopyPairingTicket?: () => Promise<void>
}

export function PairedDevicesPanel({
	pairedDevices,
	pairedInviteStatus,
	isNodeReady,
	hasTicket,
	pairingTicket,
	pairingCopySuccess,
	onInvitePairedDevice,
	onCopyPairingTicket,
}: PairedDevicesPanelProps) {
	const { t } = useTranslation()
	const [addDeviceOpen, setAddDeviceOpen] = useState(false)

	const sortedDevices = [...pairedDevices].sort((a, b) =>
		a.display_name.localeCompare(b.display_name)
	)

	const pairingDisabled = !isNodeReady || !pairingTicket
	const pairingDisplayValue = pairingTicket
		? t('common:sender.sharingActive.devices.pairingCodeReady')
		: isNodeReady
			? t('common:loading')
			: ''

	return (
		<div className="space-y-6">
			<Collapsible
				open={addDeviceOpen}
				onOpenChange={setAddDeviceOpen}
				className="rounded-lg border border-border"
			>
				<CollapsibleTrigger className="flex w-full items-center justify-between gap-2 p-3 text-left">
					<p className="text-sm font-medium">
						{t('common:sender.sharingActive.devices.addDeviceTitle')}
					</p>
					<ChevronDown
						className={cn(
							'h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200',
							addDeviceOpen && 'rotate-180'
						)}
					/>
				</CollapsibleTrigger>
				<CollapsiblePanel>
					<div className="space-y-3 border-t border-border px-3 pb-3 pt-2">
						<p className="text-xs text-muted-foreground">
							{t('common:sender.sharingActive.devices.addDeviceHint')}
						</p>

						{!isNodeReady && (
							<p className="rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground">
								{t('common:sender.sharingActive.devices.nodeUnavailable')}
							</p>
						)}

						<div className="space-y-1.5">
							<p className="text-sm font-medium">
								{t('common:sender.sharingActive.devices.pairingCodeLabel')}
							</p>
							<InputGroup>
								<InputGroupInput
									type="text"
									readOnly
									value={pairingDisplayValue}
									className="text-ellipsis font-mono text-xs"
								/>
								<InputGroupAddon align="inline-end">
									<Button
										type="button"
										size="icon-xs"
										disabled={pairingDisabled}
										onClick={onCopyPairingTicket}
										title={t(
											'common:sender.sharingActive.devices.copyPairingCode'
										)}
									>
										{pairingCopySuccess ? (
											<CheckCircle className="h-4 w-4" />
										) : (
											<Copy className="h-4 w-4" />
										)}
									</Button>
								</InputGroupAddon>
							</InputGroup>
						</div>

						<p className="text-center text-xs text-muted-foreground">
							<Link
								to="/settings/devices"
								className="underline underline-offset-2 hover:text-foreground"
							>
								{t('common:sender.sharingActive.devices.manageInSettings')}
							</Link>
						</p>
					</div>
				</CollapsiblePanel>
			</Collapsible>

			<section className="space-y-3">
				<div className="space-y-0.5">
					<p className="text-sm font-medium">
						{t('common:sender.sharingActive.devices.title')}
					</p>
					<p className="text-xs text-muted-foreground">
						{t('common:sender.sharingActive.devices.hint')}
					</p>
				</div>

				{sortedDevices.length === 0 ? (
					<p className="rounded-md border border-dashed px-3 py-4 text-center text-xs text-muted-foreground">
						{t('common:sender.sharingActive.devices.emptyTitle')}
					</p>
				) : (
					<ul className="space-y-1 max-h-48 sm:max-h-56 overflow-y-auto -mx-1 px-1">
						{sortedDevices.map((device) => {
							const Icon = deviceTypeIcon(device.device_type)
							const inviteStatus = pairedInviteStatus[device.endpoint_id]
							const disabled =
								!isNodeReady || !hasTicket || inviteStatus === 'sending'
							return (
								<li
									key={device.endpoint_id}
									className="flex items-center justify-between gap-2 rounded-lg px-1 py-1.5 text-sm"
								>
									<div className="flex min-w-0 items-center gap-2">
										<Icon className="h-4 w-4 shrink-0 text-muted-foreground" />
										<div className="min-w-0">
											<span className="block truncate">
												{device.display_name}
											</span>
											<span className="block truncate text-xs text-muted-foreground">
												{deviceSubtitle(device)}
											</span>
										</div>
									</div>
									<Button
										type="button"
										size="sm"
										variant="outline"
										disabled={disabled}
										onClick={() =>
											onInvitePairedDevice?.(device.endpoint_id)
										}
									>
										{inviteStatus === 'sending' ? (
											<>
												<Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
												{t('common:sender.pairedDevices.sending')}
											</>
										) : inviteStatus === 'sent' ? (
											<>
												<Check className="w-3.5 h-3.5 mr-1.5" />
												{t('common:sender.pairedDevices.sent')}
											</>
										) : inviteStatus === 'failed' ? (
											t('common:sender.pairedDevices.failed')
										) : (
											t('common:sender.pairedDevices.send')
										)}
									</Button>
								</li>
							)
						})}
					</ul>
				)}
			</section>
		</div>
	)
}
