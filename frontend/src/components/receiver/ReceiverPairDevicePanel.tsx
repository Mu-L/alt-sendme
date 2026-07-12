import { useState } from 'react'
import { Link } from 'react-router-dom'
import { ChevronDown, Loader2 } from 'lucide-react'
import { useTranslation } from '../../i18n/react-i18next-compat'
import { usePairing } from '../../hooks/usePairing'
import { toastManager } from '../ui/toast'
import { Button } from '../ui/button'
import { Input } from '../ui/input'
import { Label } from '../ui/label'
import {
	Collapsible,
	CollapsiblePanel,
	CollapsibleTrigger,
} from '../ui/collapsible'
import { cn } from '@/lib/utils'

export function ReceiverPairDevicePanel() {
	const { t } = useTranslation()
	const { join, isJoining, isNodeReady } = usePairing()
	const [code, setCode] = useState('')
	const [pairOpen, setPairOpen] = useState(false)

	const handleJoin = async () => {
		if (!code.trim()) return
		try {
			await join(code)
			setCode('')
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
		<Collapsible
			open={pairOpen}
			onOpenChange={setPairOpen}
			className="rounded-lg border border-border"
		>
			<CollapsibleTrigger className="flex w-full items-center justify-between gap-2 p-3 text-left">
				<p className="text-sm font-medium">
					{t('common:receiver.pairDevice.title')}
				</p>
				<ChevronDown
					className={cn(
						'h-4 w-4 shrink-0 text-muted-foreground transition-transform duration-200',
						pairOpen && 'rotate-180'
					)}
				/>
			</CollapsibleTrigger>
			<CollapsiblePanel>
				<div className="space-y-3 border-t border-border px-3 pb-3 pt-2">
					<p className="text-xs text-muted-foreground">
						{t('common:receiver.pairDevice.hint')}
					</p>

					{!isNodeReady && (
						<p className="rounded-md border border-dashed px-3 py-2 text-xs text-muted-foreground">
							{t('common:settings.devices.nodeUnavailableTitle')}
						</p>
					)}

					<div className="space-y-2">
						<Label htmlFor="receiver-pairing-code">
							{t('common:receiver.pairDevice.codeLabel')}
						</Label>
						<div className="flex gap-2">
							<Input
								id="receiver-pairing-code"
								value={code}
								onChange={(event: React.ChangeEvent<HTMLInputElement>) =>
									setCode(event.target.value)
								}
								onKeyDown={(event: React.KeyboardEvent<HTMLInputElement>) => {
									if (event.key === 'Enter' && code.trim() && !isJoining) {
										event.preventDefault()
										void handleJoin()
									}
								}}
								placeholder={t('common:receiver.pairDevice.placeholder')}
								className="font-mono text-xs min-w-0"
								disabled={!isNodeReady || isJoining}
							/>
							<Button
								type="button"
								size="sm"
								className="shrink-0"
								disabled={!isNodeReady || isJoining || !code.trim()}
								onClick={handleJoin}
							>
								{isJoining ? (
									<>
										<Loader2 className="w-4 h-4 mr-1.5 animate-spin" />
										{t('common:receiver.pairDevice.pairing')}
									</>
								) : (
									t('common:receiver.pairDevice.pairButton')
								)}
							</Button>
						</div>
					</div>

					<p className="text-center text-xs text-muted-foreground">
						<Link
							to="/settings/devices"
							className="underline underline-offset-2 hover:text-foreground"
						>
							{t('common:receiver.pairDevice.manageInSettings')}
						</Link>
					</p>
				</div>
			</CollapsiblePanel>
		</Collapsible>
	)
}
