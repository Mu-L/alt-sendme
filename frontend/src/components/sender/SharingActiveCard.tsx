import { Check, CheckCircle, Copy, Loader2, Square } from 'lucide-react'
import { useTranslation } from '../../i18n/react-i18next-compat'
import type {
	SharingControlsProps,
	TicketDisplayProps,
} from '../../types/sender'
import { TransferProgressBar } from '../common/TransferProgressBar'
import { StatusIndicator } from '../common/StatusIndicator'
import { Button } from '../ui/button'
import { InputGroup, InputGroupAddon, InputGroupInput } from '../ui/input-group'
import { Label } from '../ui/label'
import { Switch } from '../ui/switch'
import { toastManager } from '../ui/toast'
import { Tooltip, TooltipContent, TooltipTrigger } from '../ui/tooltip'
import { useAppSettingStore } from '../../store/app-setting'
import { deviceSubtitle } from '@/lib/pairing-api'
import { deviceTypeIcon } from '@/lib/device-icon'

export function SharingActiveCard({
	selectedPaths,
	selectedPath,
	ticket,
	copySuccess,
	transferProgress,
	isTransporting,
	isCompleted,
	isBroadcastMode,
	activeConnectionCount = 0,
	pairedDevices = [],
	isNodeReady = false,
	pairedInviteStatus = {},
	onInvitePairedDevice,
	onCopyTicket,
	onStopSharing,
	onSetBroadcast: _onSetBroadcast,
}: SharingControlsProps) {
	const { t } = useTranslation()
	const onSetBroadcast = () => {
		if (_onSetBroadcast) {
			const isTurningOn = !isBroadcastMode
			_onSetBroadcast(isTurningOn)
			if (isTurningOn) {
				const toastId = crypto.randomUUID()
				toastManager.add({
					title: t('common:sender.broadcastMode.on.label'),
					id: toastId,
					description: t('common:sender.broadcastMode.on.description'),
					type: 'info',
					actionProps: {
						children: t('common:undo'),
						onClick: () => {
							_onSetBroadcast?.(false)
							toastManager.close(toastId)
						},
					},
				})
				setTimeout(() => {
					toastManager.close(toastId)
				}, 5000)
			}
		}
	}

	const getStatusText = () => {
		if (isCompleted) return t('common:sender.transferCompleted')
		if (isTransporting) return t('common:sender.sharingInProgress')
		return t('common:sender.listeningForConnection')
	}

	const statusText = getStatusText()

	const clampedProgress = transferProgress
		? {
				...transferProgress,
				bytesTransferred: Math.min(
					Math.max(transferProgress.bytesTransferred, 0),
					transferProgress.totalBytes
				),
				percentage: Math.min(Math.max(transferProgress.percentage, 0), 100),
			}
		: null

	const defaultProgress = {
		bytesTransferred: 0,
		totalBytes: 0,
		speedBps: 0,
		percentage: 0,
	}

	const progressToDisplay = isTransporting
		? clampedProgress || defaultProgress
		: null

	const showPairedSend =
		!isTransporting &&
		ticket &&
		isNodeReady &&
		pairedDevices.length > 0 &&
		onInvitePairedDevice

	return (
		<div className="space-y-4">
			<div className="p-4 rounded-lg absolute top-0 left-0">
				<Tooltip disabled={!selectedPath && selectedPaths.length <= 1}>
					<TooltipTrigger>
						<p className="text-xs mb-4 max-w-40 sm:max-w-120 truncate">
							<strong className="mr-1">{t('common:sender.fileLabel')}</strong>{' '}
							{selectedPaths.length > 1
								? t('common:sender.multipleFilesSelected', {
										name: selectedPaths[0]?.split('/').pop() || '',
										count: selectedPaths.length - 1,
									})
								: selectedPath?.split('/').pop()}
						</p>
					</TooltipTrigger>
					<TooltipContent className="max-w-xs" side="inline-end">
						<ul className="list-disc pl-4 text-left max-h-60 overflow-auto">
							{selectedPaths.map((path) => (
								<li key={path} className="text-xs">
									{path.split('/').pop()}
								</li>
							))}
						</ul>
					</TooltipContent>
				</Tooltip>

				<StatusIndicator
					isCompleted={isCompleted}
					isTransporting={isTransporting}
					statusText={statusText}
					activeConnectionCount={activeConnectionCount}
					isBroadcastMode={isBroadcastMode}
				/>
			</div>

			<p className="text-xs text-center">{t('common:sender.keepAppOpen')}</p>

			{!isTransporting && ticket && (
				<TicketDisplay
					ticket={ticket}
					copySuccess={copySuccess}
					onCopyTicket={onCopyTicket}
					isBroadcastMode={isBroadcastMode}
					onSetBroadcast={onSetBroadcast}
				/>
			)}

			{showPairedSend && (
				<div className="space-y-2">
					<p className="text-sm font-medium">
						{t('common:sender.pairedDevices.yourDevicesTitle')}
					</p>
					<ul className="space-y-2">
						{pairedDevices.map((device) => {
							const Icon = deviceTypeIcon(device.device_type)
							const inviteStatus = pairedInviteStatus[device.endpoint_id]
							return (
								<li
									key={device.endpoint_id}
									className="flex items-center justify-between gap-2 text-sm"
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
										disabled={inviteStatus === 'sending'}
										onClick={() => onInvitePairedDevice(device.endpoint_id)}
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
										) : (
											t('common:sender.pairedDevices.send')
										)}
									</Button>
								</li>
							)
						})}
					</ul>
				</div>
			)}

			{isTransporting && progressToDisplay && (
				<TransferProgressBar progress={progressToDisplay} />
			)}

			<Button
				size="icon-lg"
				type="button"
				onClick={onStopSharing}
				variant="destructive-outline"
				className="absolute top-0 right-0 sm:right-6 rounded-full font-medium transition-colors not-disabled:not-active:not-data-pressed:before:shadow-none dark:not-disabled:before:shadow-none dark:not-disabled:not-active:not-data-pressed:before:shadow-none"
				aria-label="Stop sharing"
			>
				<Square className="w-4 h-4" fill="currentColor" />
			</Button>
		</div>
	)
}

export function TicketDisplay({
	ticket,
	copySuccess,
	onCopyTicket,
	isBroadcastMode,
	onSetBroadcast,
}: TicketDisplayProps & {
	isBroadcastMode?: boolean
	onSetBroadcast?: (broadcast: boolean) => void
}) {
	const { t } = useTranslation()
	const showBroadcastToggle = useAppSettingStore(
		(state) => state.showBroadcastToggle
	)

	return (
		<div className="space-y-3">
			<div className="flex items-center justify-between">
				<p className="block text-sm font-medium">
					{t('common:sender.shareThisTicket')}
				</p>
				{showBroadcastToggle &&
					isBroadcastMode !== undefined &&
					onSetBroadcast && (
						<div className="flex items-start gap-2">
							<Label htmlFor={'broadcast-toggle'} className="text-sm">
								{t('common:sender.broadcastMode.index')}
							</Label>
							<Switch
								checked={isBroadcastMode}
								onCheckedChange={onSetBroadcast}
							/>
						</div>
					)}
			</div>
			<InputGroup>
				<InputGroupInput
					type="text"
					value={ticket}
					className="overflow-ellipsis"
					readOnly
				/>
				<InputGroupAddon align="inline-end">
					<Button
						type="button"
						size="icon-xs"
						onClick={onCopyTicket}
						style={{
							backgroundColor: copySuccess
								? 'var(--app-primary)'
								: 'var(--color-foreground)',
							border: '1px solid var(--border)',
						}}
						title={t('common:sender.copyToClipboard')}
					>
						{copySuccess ? (
							<CheckCircle className="h-4 w-4" />
						) : (
							<Copy className="h-4 w-4" />
						)}
					</Button>
				</InputGroupAddon>
			</InputGroup>
			<p className="text-xs text-muted-foreground">
				{t('common:sender.sendThisTicket')}
			</p>
		</div>
	)
}
