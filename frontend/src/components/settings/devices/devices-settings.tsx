import { useEffect, useState } from 'react'
import { Copy, Loader2, Pencil, Plus, QrCode, Trash2 } from 'lucide-react'
import { useTranslation } from '../../../i18n'
import { usePairing } from '../../../hooks/usePairing'
import { Button } from '../../ui/button'
import {
	AlertDialog,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
} from '../../ui/alert-dialog'
import { Input } from '../../ui/input'
import { Label } from '../../ui/label'
import { Textarea } from '../../ui/textarea'
import { toastManager } from '../../ui/toast'
import {
	Frame,
	FrameDescription,
	FramePanel,
	FrameTitle,
} from '../../ui/frame'
import { IS_DESKTOP } from '@/lib/platform'
import { deviceSubtitle } from '@/lib/pairing-api'
import { deviceTypeIcon } from '@/lib/device-icon'

function PairHostModal({
	open,
	ticket,
	isLoading,
	expiresIn,
	onDismiss,
	onCancelPairing,
}: {
	open: boolean
	ticket: string | null
	isLoading: boolean
	expiresIn: number | null
	/** Hide the dialog but keep the pairing window open until TTL. */
	onDismiss: () => void
	/** Abort pairing immediately. */
	onCancelPairing: () => void
}) {
	const { t } = useTranslation()

	const copyTicket = async () => {
		if (!ticket) return
		await navigator.clipboard.writeText(ticket)
		toastManager.add({
			title: t('common:settings.devices.copied'),
			type: 'success',
		})
	}

	return (
		<AlertDialog
			open={open}
			onOpenChange={(next) => {
				// Dismissing the dialog must NOT stop pairing — the peer still
				// needs the host window open after the code is copied.
				if (!next) onDismiss()
			}}
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>
						{t('common:settings.devices.showQrCode')}
					</AlertDialogTitle>
					<AlertDialogDescription>
						{t('common:settings.devices.hostHint')}
						{expiresIn != null && expiresIn > 0 ? (
							<>
								{' '}
								{t('common:settings.devices.hostExpiresIn', {
									seconds: expiresIn,
								})}
							</>
						) : null}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<div className="space-y-3 px-6">
					<Label>{t('common:settings.devices.pairingCode')}</Label>
					<Textarea
						readOnly
						value={isLoading ? t('common:loading') : (ticket ?? '')}
						className="font-mono text-xs min-h-28"
					/>
					<Button
						type="button"
						variant="outline"
						disabled={isLoading || !ticket}
						onClick={copyTicket}
					>
						<Copy className="w-4 h-4 mr-2" />
						{t('common:sender.copyToClipboard')}
					</Button>
				</div>
				<AlertDialogFooter>
					<Button
						type="button"
						variant="outline"
						onClick={onCancelPairing}
					>
						{t('common:settings.devices.cancelPairing')}
					</Button>
					<Button type="button" onClick={onDismiss}>
						{t('common:ok')}
					</Button>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	)
}

function PairJoinModal({
	open,
	isLoading,
	onClose,
	onJoin,
}: {
	open: boolean
	isLoading: boolean
	onClose: () => void
	onJoin: (ticket: string) => Promise<void>
}) {
	const { t } = useTranslation()
	const [code, setCode] = useState('')

	const handleJoin = async () => {
		if (!code.trim()) return
		try {
			await onJoin(code)
			setCode('')
			onClose()
			toastManager.add({
				title: t('common:settings.devices.devicePaired'),
				type: 'success',
			})
		} catch (error) {
			console.error(error)
			toastManager.add({
				title: t('common:settings.devices.pairFailed'),
				type: 'error',
			})
		}
	}

	return (
		<AlertDialog
			open={open}
			onOpenChange={(next) => {
				if (!next) {
					setCode('')
					onClose()
				}
			}}
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>
						{t('common:settings.devices.enterCode')}
					</AlertDialogTitle>
					<AlertDialogDescription>
						{t('common:settings.devices.joinHint')}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<div className="px-6 pb-2">
					<Input
						value={code}
						onChange={(event: React.ChangeEvent<HTMLInputElement>) =>
							setCode(event.target.value)
						}
						placeholder={t('common:settings.devices.codePlaceholder')}
						className="font-mono text-xs"
					/>
				</div>
				<AlertDialogFooter>
					<Button type="button" variant="outline" onClick={onClose}>
						{t('common:cancel')}
					</Button>
					<Button
						type="button"
						disabled={isLoading || !code.trim()}
						onClick={handleJoin}
					>
						{isLoading ? (
							<>
								<Loader2 className="w-4 h-4 mr-2 animate-spin" />
								{t('common:settings.devices.pairing')}
							</>
						) : (
							t('common:settings.devices.pairDevice')
						)}
					</Button>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	)
}

function RenameDeviceModal({
	open,
	title,
	description,
	initialName,
	onClose,
	onSave,
}: {
	open: boolean
	title: string
	description: string
	initialName: string
	onClose: () => void
	onSave: (name: string) => Promise<void>
}) {
	const { t } = useTranslation()
	const [name, setName] = useState(initialName)
	const [saving, setSaving] = useState(false)

	useEffect(() => {
		if (open) setName(initialName)
	}, [open, initialName])

	const handleSave = async () => {
		const trimmed = name.trim()
		if (!trimmed) return
		setSaving(true)
		try {
			await onSave(trimmed)
			onClose()
			toastManager.add({
				title: t('common:settings.devices.nameSaved'),
				type: 'success',
			})
		} catch (error) {
			console.error(error)
			toastManager.add({
				title: t('common:settings.devices.nameSaveFailed'),
				type: 'error',
			})
		} finally {
			setSaving(false)
		}
	}

	return (
		<AlertDialog
			open={open}
			onOpenChange={(next) => {
				if (!next) onClose()
			}}
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>{title}</AlertDialogTitle>
					<AlertDialogDescription>{description}</AlertDialogDescription>
				</AlertDialogHeader>
				<div className="px-6 pb-2">
					<Label htmlFor="device-display-name">
						{t('common:settings.devices.deviceName')}
					</Label>
					<Input
						id="device-display-name"
						value={name}
						maxLength={64}
						onChange={(event: React.ChangeEvent<HTMLInputElement>) =>
							setName(event.target.value)
						}
						placeholder={t('common:settings.devices.deviceNamePlaceholder')}
						className="mt-2"
						autoFocus
					/>
				</div>
				<AlertDialogFooter>
					<Button type="button" variant="outline" onClick={onClose}>
						{t('common:cancel')}
					</Button>
					<Button
						type="button"
						disabled={saving || !name.trim()}
						onClick={handleSave}
					>
						{t('common:settings.devices.saveName')}
					</Button>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	)
}

export function DevicesSettings() {
	const { t } = useTranslation()
	const {
		devices,
		thisDevice,
		pairingTicket,
		hostExpiresIn,
		isJoining,
		isLoading,
		hostPairedCount,
		isNodeReady,
		nodeStatus,
		openHostPairing,
		closeHostPairing,
		join,
		forget,
		renameThisDevice,
		renameDevice,
	} = usePairing()
	const [hostOpen, setHostOpen] = useState(false)
	const [joinOpen, setJoinOpen] = useState(false)
	const [renameThisOpen, setRenameThisOpen] = useState(false)
	const [renamePeerId, setRenamePeerId] = useState<string | null>(null)

	// When a peer joins our open pairing window, close the QR dialog and
	// confirm success instead of leaving the stale code on screen.
	useEffect(() => {
		if (hostPairedCount === 0) return
		setHostOpen(false)
		toastManager.add({
			title: t('common:settings.devices.devicePaired'),
			type: 'success',
		})
	}, [hostPairedCount, t])

	if (!IS_DESKTOP) {
		return (
			<Frame>
				<FramePanel>
					<div className="space-y-2">
						<FrameTitle>{t('common:settings.devices.title')}</FrameTitle>
						<FrameDescription>
							{t('common:settings.devices.desktopOnly')}
						</FrameDescription>
					</div>
				</FramePanel>
			</Frame>
		)
	}

	if (!isNodeReady) {
		return (
			<Frame>
				<FramePanel>
					<div className="space-y-2">
						<FrameTitle>{t('common:settings.devices.title')}</FrameTitle>
						<FrameDescription>
							{t('common:settings.devices.nodeUnavailableTitle')}
						</FrameDescription>
						<p className="text-sm text-muted-foreground">
							{nodeStatus.reason
								? nodeStatus.reason
								: t('common:settings.devices.nodeUnavailableHint')}
						</p>
					</div>
				</FramePanel>
			</Frame>
		)
	}

	const openHost = async () => {
		setHostOpen(true)
		try {
			await openHostPairing()
		} catch (error) {
			console.error(error)
			setHostOpen(false)
			toastManager.add({
				title: t('common:settings.devices.pairFailed'),
				type: 'error',
			})
		}
	}

	const dismissHost = () => {
		setHostOpen(false)
	}

	const cancelHost = async () => {
		setHostOpen(false)
		await closeHostPairing()
	}

	const renamePeer = devices.find((d) => d.endpoint_id === renamePeerId)
	const ThisDeviceIcon = deviceTypeIcon(thisDevice?.device_type)

	return (
		<>
			{thisDevice ? (
				<Frame className="mb-4">
					<FramePanel className="flex flex-col gap-4">
						<div className="space-y-1">
							<FrameTitle>{t('common:settings.devices.thisDevice')}</FrameTitle>
							<FrameDescription>
								{t('common:settings.devices.thisDeviceHint')}
							</FrameDescription>
						</div>
						<div className="flex items-center justify-between gap-3">
							<div className="flex min-w-0 items-center gap-3">
								<div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
									<ThisDeviceIcon className="h-5 w-5" />
								</div>
								<div className="min-w-0">
									<p className="font-medium truncate">
										{thisDevice.display_name}
									</p>
									<p className="text-xs text-muted-foreground truncate">
										{deviceSubtitle(thisDevice)}
									</p>
								</div>
							</div>
							<Button
								type="button"
								size="sm"
								variant="outline"
								onClick={() => setRenameThisOpen(true)}
							>
								<Pencil className="w-4 h-4 mr-2" />
								{t('common:settings.devices.rename')}
							</Button>
						</div>
					</FramePanel>
				</Frame>
			) : null}

			<Frame>
				<FramePanel className="flex flex-col gap-6">
					<div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
						<div className="space-y-2 min-w-0">
							<FrameTitle>{t('common:settings.devices.title')}</FrameTitle>
							<FrameDescription>
								{t('common:settings.devices.description')}
							</FrameDescription>
						</div>
						<div className="flex flex-wrap gap-2 shrink-0">
							<Button
								type="button"
								size="sm"
								disabled={isLoading && hostOpen}
								onClick={openHost}
							>
								<QrCode className="w-4 h-4 mr-2" />
								{t('common:settings.devices.showQrCode')}
							</Button>
							<Button
								type="button"
								size="sm"
								variant="outline"
								onClick={() => setJoinOpen(true)}
							>
								<Plus className="w-4 h-4 mr-2" />
								{t('common:settings.devices.enterCode')}
							</Button>
						</div>
					</div>

					{pairingTicket && hostExpiresIn != null && hostExpiresIn > 0 && !hostOpen ? (
						<div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-dashed px-3 py-2 text-sm">
							<p className="text-muted-foreground">
								{t('common:settings.devices.hostStillOpen', {
									seconds: hostExpiresIn,
								})}
							</p>
							<div className="flex gap-2">
								<Button
									type="button"
									size="sm"
									variant="outline"
									onClick={() => setHostOpen(true)}
								>
									{t('common:settings.devices.showQrCode')}
								</Button>
								<Button
									type="button"
									size="sm"
									variant="ghost"
									onClick={cancelHost}
								>
									{t('common:settings.devices.cancelPairing')}
								</Button>
							</div>
						</div>
					) : null}

					{devices.length === 0 ? (
						<div className="py-8 text-center text-sm text-muted-foreground border-t">
							<p className="font-medium text-foreground mb-2">
								{t('common:settings.devices.noPairedDevices')}
							</p>
							<p>{t('common:settings.devices.noPairedDevicesHint')}</p>
						</div>
					) : (
						<ul className="divide-y border-t">
							{devices.map((device) => {
								const Icon = deviceTypeIcon(device.device_type)
								return (
									<li
										key={device.endpoint_id}
										className="flex items-center justify-between gap-3 py-3 first:pt-4"
									>
										<div className="flex min-w-0 items-center gap-3">
											<div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground">
												<Icon className="h-4 w-4" />
											</div>
											<div className="min-w-0">
												<p className="font-medium truncate">
													{device.display_name}
												</p>
												<p className="text-xs text-muted-foreground truncate">
													{deviceSubtitle(device)}
												</p>
											</div>
										</div>
										<div className="flex shrink-0 items-center gap-1">
											<Button
												type="button"
												variant="ghost"
												size="icon-sm"
												aria-label={t('common:settings.devices.rename')}
												onClick={() => setRenamePeerId(device.endpoint_id)}
											>
												<Pencil className="w-4 h-4" />
											</Button>
											<Button
												type="button"
												variant="ghost"
												size="icon-sm"
												aria-label={t('common:settings.devices.removeDevice')}
												onClick={async () => {
													try {
														await forget(device.endpoint_id)
														toastManager.add({
															title: t(
																'common:settings.devices.deviceRemoved'
															),
															type: 'success',
														})
													} catch (error) {
														console.error(error)
														toastManager.add({
															title: t(
																'common:settings.devices.removeFailed'
															),
															type: 'error',
														})
													}
												}}
											>
												<Trash2 className="w-4 h-4" />
											</Button>
										</div>
									</li>
								)
							})}
						</ul>
					)}
				</FramePanel>
			</Frame>

			<PairHostModal
				open={hostOpen}
				ticket={pairingTicket}
				isLoading={isLoading}
				expiresIn={hostExpiresIn}
				onDismiss={dismissHost}
				onCancelPairing={cancelHost}
			/>
			<PairJoinModal
				open={joinOpen}
				isLoading={isJoining}
				onClose={() => setJoinOpen(false)}
				onJoin={join}
			/>
			{thisDevice ? (
				<RenameDeviceModal
					open={renameThisOpen}
					title={t('common:settings.devices.renameThisDevice')}
					description={t('common:settings.devices.renameThisDeviceHint')}
					initialName={thisDevice.display_name}
					onClose={() => setRenameThisOpen(false)}
					onSave={async (name) => {
						await renameThisDevice(name)
					}}
				/>
			) : null}
			{renamePeer ? (
				<RenameDeviceModal
					open
					title={t('common:settings.devices.renameDevice')}
					description={t('common:settings.devices.renameDeviceHint')}
					initialName={renamePeer.display_name}
					onClose={() => setRenamePeerId(null)}
					onSave={async (name) => {
						await renameDevice(renamePeer.endpoint_id, name)
					}}
				/>
			) : null}
		</>
	)
}
