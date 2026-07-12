import { useEffect, useRef, useState } from 'react'
import { useTranslation } from '../../i18n/react-i18next-compat'
import type { SharingControlsProps } from '../../types/sender'
import { IS_DESKTOP } from '@/lib/platform'
import {
	Tabs,
	TabsContent,
	TabsList,
	TabsTrigger,
} from '../animate-ui/components/tabs'
import { Button } from '../ui/button'
import { PairedDevicesPanel } from './PairedDevicesPanel'
import { ShareLinkPanel } from './ShareLinkPanel'

type SharingTab = 'devices' | 'link'

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
	pairingTicket = null,
	pairingCopySuccess = false,
	onInvitePairedDevice,
	onCopyPairingTicket,
	onCopyTicket,
	onStopSharing,
	onSetBroadcast,
}: SharingControlsProps) {
	const { t } = useTranslation()

	const [activeTab, setActiveTab] = useState<SharingTab>(
		!isBroadcastMode ? 'devices' : 'link'
	)

	// When a transfer starts (e.g. after inviting a paired device), surface the
	// progress by moving to the Share link tab where the progress bar lives.
	const prevTransporting = useRef(isTransporting)
	useEffect(() => {
		if (
			isTransporting &&
			!prevTransporting.current &&
			activeTab === 'devices'
		) {
			setActiveTab('link')
		}
		prevTransporting.current = isTransporting
	}, [isTransporting, activeTab])

	const shareLinkPanel = (
		<ShareLinkPanel
			selectedPaths={selectedPaths}
			selectedPath={selectedPath}
			ticket={ticket}
			copySuccess={copySuccess}
			isTransporting={isTransporting}
			isCompleted={isCompleted}
			isBroadcastMode={isBroadcastMode}
			activeConnectionCount={activeConnectionCount}
			transferProgress={transferProgress}
			onCopyTicket={onCopyTicket}
			onSetBroadcast={onSetBroadcast}
			onStopSharing={onStopSharing}
		/>
	)

	const showTabs = IS_DESKTOP

	return (
		<div className="flex flex-col h-full">
			{showTabs ? (
				<Tabs
					value={activeTab}
					onValueChange={(v) => setActiveTab(v as SharingTab)}
					className="flex-1 min-h-0"
				>
					<TabsList className="w-full">
						<TabsTrigger value="devices">
							{t('common:sender.sharingActive.tabs.devices')}
						</TabsTrigger>
						<TabsTrigger value="link">
							{t('common:sender.sharingActive.tabs.link')}
						</TabsTrigger>
					</TabsList>
					<TabsContent
						value="devices"
						className="flex flex-col flex-1 min-h-0 pt-4"
					>
						<div className="flex-1 min-h-0 overflow-y-auto">
							<PairedDevicesPanel
								pairedDevices={pairedDevices}
								pairedInviteStatus={pairedInviteStatus}
								isNodeReady={isNodeReady}
								hasTicket={Boolean(ticket)}
								pairingTicket={pairingTicket}
								pairingCopySuccess={pairingCopySuccess}
								onInvitePairedDevice={onInvitePairedDevice}
								onCopyPairingTicket={onCopyPairingTicket}
							/>
						</div>
						<div className="shrink-0 border-t border-border pt-3 mt-4">
							<Button
								type="button"
								variant="outline"
								className="w-full"
								onClick={onStopSharing}
							>
								{t('common:sender.exitSharing')}
							</Button>
						</div>
					</TabsContent>
					<TabsContent value="link" className="pt-4">
						{shareLinkPanel}
					</TabsContent>
				</Tabs>
			) : (
				shareLinkPanel
			)}
		</div>
	)
}
