import { useTranslation } from '../../i18n/react-i18next-compat'
import { formatReceiveSavePath } from '@/lib/receive-save-path'
import { IS_ANDROID, IS_WEB } from '@/lib/platform'
import { supportsWebSaveLocationPicker } from '@/lib/platform-api'
import { InputGroup, InputGroupAddon, InputGroupInput } from '../ui/input-group'
import { Button } from '../ui/button'

interface ReceiveSaveLocationPickerProps {
	savePath: string
	disabled?: boolean
	onBrowseFolder: () => Promise<void>
}

export function ReceiveSaveLocationPicker({
	savePath,
	disabled = false,
	onBrowseFolder,
}: ReceiveSaveLocationPickerProps) {
	const { t } = useTranslation()
	const canPickSaveLocation = IS_WEB ? supportsWebSaveLocationPicker() : true
	const saveLocationLabel = IS_WEB
		? t('common:receiver.saveLocation')
		: t('common:receiver.saveToFolder')
	const noSaveLocationText =
		IS_WEB && !canPickSaveLocation
			? t('common:receiver.browserDownloadsFallback')
			: IS_ANDROID
				? t('common:receiver.appDownloadsDefault')
				: t('common:receiver.noFolderSelected')
	const saveLocationHint =
		IS_WEB && !canPickSaveLocation
			? t('common:receiver.browserDownloadsHint')
			: IS_ANDROID && !savePath
				? t('common:receiver.appDownloadsHint')
				: null

	return (
		<div>
			<p className="block text-sm font-medium mb-2">{saveLocationLabel}</p>
			<InputGroup
				onClick={canPickSaveLocation && !disabled ? onBrowseFolder : undefined}
				className={
					canPickSaveLocation && !disabled ? undefined : 'cursor-default'
				}
			>
				<InputGroupInput
					disabled
					value={formatReceiveSavePath(savePath) || noSaveLocationText}
					className="text-ellipsis"
				/>
				{canPickSaveLocation ? (
					<InputGroupAddon align="inline-end">
						<Button
							type="button"
							disabled={disabled}
							size="xs"
							onClick={(event) => {
								event.stopPropagation()
								void onBrowseFolder()
							}}
						>
							{t('common:browse')}
						</Button>
					</InputGroupAddon>
				) : null}
			</InputGroup>
			{saveLocationHint ? (
				<p className="mt-1.5 text-xs text-muted-foreground">
					{saveLocationHint}
				</p>
			) : null}
		</div>
	)
}
