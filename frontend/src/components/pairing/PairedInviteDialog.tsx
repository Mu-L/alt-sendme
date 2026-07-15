import { useLocation, useNavigate } from 'react-router-dom'
import { useTranslation } from '@/i18n'
import { IS_ANDROID, IS_PAIRING_CAPABLE, IS_WEB } from '@/lib/platform'
import { formatReceiveSavePath } from '@/lib/receive-save-path'
import { supportsWebSaveLocationPicker } from '@/lib/platform-api'
import { formatFileSize } from '@/lib/utils'
import { respondPairedInvite } from '@/lib/pairing-api'
import { usePairedInviteStore } from '@/store/paired-invite-store'
import { useReceiverActionsStore } from '@/store/receiver-actions-store'
import {
	AlertDialog,
	AlertDialogContent,
	AlertDialogDescription,
	AlertDialogFooter,
	AlertDialogHeader,
	AlertDialogTitle,
	AlertDialogClose,
} from '../ui/alert-dialog'
import { Button } from '../ui/button'
import { toastManager } from '../ui/toast'

export function PairedInviteDialog() {
	const { t } = useTranslation()
	const navigate = useNavigate()
	const location = useLocation()
	const invite = usePairedInviteStore((s) => s.invite)
	const setInvite = usePairedInviteStore((s) => s.setInvite)
	const acceptPairedInvite = useReceiverActionsStore(
		(s) => s.acceptPairedInvite
	)
	const browseSaveFolder = useReceiverActionsStore((s) => s.browseSaveFolder)
	const savePath = useReceiverActionsStore((s) => s.savePath)

	const notifyInviteResponse = (endpointId: string, accepted: boolean) => {
		void respondPairedInvite(endpointId, accepted).catch(() => {
			// Best-effort notify; accept/decline UI already proceeded.
		})
	}

	const decline = () => {
		const current = usePairedInviteStore.getState().invite
		if (!current) return
		setInvite(null)
		notifyInviteResponse(current.remote_endpoint_id, false)
	}

	const accept = async () => {
		const current = usePairedInviteStore.getState().invite
		if (!current) return
		if (!acceptPairedInvite) {
			toastManager.add({
				title: t('common:errors.receiveFailed'),
				description: t('common:receiver.openReceiveTabHint'),
				type: 'warning',
			})
			return
		}

		setInvite(null)
		notifyInviteResponse(current.remote_endpoint_id, true)
		if (location.pathname !== '/') {
			navigate('/')
		}
		try {
			await acceptPairedInvite(current)
		} catch {
			// receiveWithTicket / accept path surfaces its own errors
		}
	}

	const changeFolder = async () => {
		if (!browseSaveFolder) {
			toastManager.add({
				title: t('common:errors.receiveFailed'),
				description: t('common:receiver.openReceiveTabHint'),
				type: 'warning',
			})
			return
		}
		await browseSaveFolder()
	}

	if (!IS_PAIRING_CAPABLE) return null

	const canPickSaveLocation = IS_WEB ? supportsWebSaveLocationPicker() : true
	const savePathDisplay = formatReceiveSavePath(savePath)
	const noSaveLocationText =
		IS_WEB && !canPickSaveLocation
			? t('common:receiver.browserDownloadsFallback')
			: IS_ANDROID
				? t('common:receiver.appDownloadsDefault')
				: t('common:receiver.noFolderSelected')

	return (
		<AlertDialog
			open={invite != null}
			onOpenChange={(open) => {
				if (!open) decline()
			}}
		>
			<AlertDialogContent>
				<AlertDialogHeader>
					<AlertDialogTitle>
						{t('common:receiver.receiveFromPairedTitle')}
					</AlertDialogTitle>
					<AlertDialogDescription>
						{invite
							? invite.total_size > 0
								? t('common:receiver.receiveFromPairedDescription', {
										sender: invite.sender_name,
										count: invite.file_count,
										size: formatFileSize(invite.total_size),
									})
								: t('common:receiver.receiveFromPairedDescriptionNoSize', {
										sender: invite.sender_name,
										count: invite.file_count,
									})
							: ''}
					</AlertDialogDescription>
				</AlertDialogHeader>
				<div className="space-y-2 px-6 pb-4">
					<p className="text-xs font-medium text-muted-foreground">
						{t('common:receiver.pairedInvite.saveTo')}
					</p>
					<div className="flex items-center gap-2 rounded-md border bg-muted/30 px-3 py-2">
						<p className="min-w-0 flex-1 truncate font-mono text-xs">
							{savePathDisplay || noSaveLocationText}
						</p>
						{canPickSaveLocation ? (
							<Button
								type="button"
								variant="outline"
								size="xs"
								className="shrink-0"
								onClick={changeFolder}
							>
								{t('common:receiver.pairedInvite.changeFolder')}
							</Button>
						) : null}
					</div>
				</div>
				<AlertDialogFooter>
					<AlertDialogClose
						render={
							<Button size="sm" variant="outline">
								{t('common:receiver.declineInvite')}
							</Button>
						}
						onClick={decline}
					/>
					<Button size="sm" onClick={accept}>
						{t('common:receiver.acceptInvite')}
					</Button>
				</AlertDialogFooter>
			</AlertDialogContent>
		</AlertDialog>
	)
}
