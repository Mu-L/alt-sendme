import { useTranslation } from '@/i18n'
import { IS_DESKTOP, IS_WEB } from '@/lib/platform'
import { formatReceiveSavePath } from '@/lib/receive-save-path'
import { supportsWebSaveLocationPicker } from '@/lib/platform-api'
import { formatFileSize } from '@/lib/utils'
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
	const invite = usePairedInviteStore((s) => s.invite)
	const setInvite = usePairedInviteStore((s) => s.setInvite)
	const acceptPairedInvite = useReceiverActionsStore(
		(s) => s.acceptPairedInvite
	)
	const browseSaveFolder = useReceiverActionsStore((s) => s.browseSaveFolder)
	const savePath = useReceiverActionsStore((s) => s.savePath)

	const decline = () => {
		setInvite(null)
	}

	const accept = async () => {
		if (!invite) return
		console.log('[paired-invite] receiver: dialog accept clicked', {
			sender: invite.sender_name,
			hasHandler: Boolean(acceptPairedInvite),
		})
		if (!acceptPairedInvite) {
			console.warn(
				'[paired-invite] receiver: accept handler not registered (Receive tab may be unmounted)'
			)
			toastManager.add({
				title: t('common:errors.receiveFailed'),
				description: t('common:receiver.openReceiveTabHint'),
				type: 'warning',
			})
			return
		}

		const payload = invite
		setInvite(null)
		try {
			await acceptPairedInvite(payload)
			console.log('[paired-invite] receiver: accept handler completed')
		} catch (error) {
			console.error('[paired-invite] receiver: accept handler failed', error)
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

	if (!IS_DESKTOP) return null

	const canPickSaveLocation = IS_WEB ? supportsWebSaveLocationPicker() : true
	const savePathDisplay = formatReceiveSavePath(savePath)
	const noSaveLocationText =
		IS_WEB && !canPickSaveLocation
			? t('common:receiver.browserDownloadsFallback')
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
				<div className="space-y-2 px-6">
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
