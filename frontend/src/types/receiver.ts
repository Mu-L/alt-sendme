import type { AlertDialogState } from './ui'
import type { TicketPreviewMetadata } from './transfer'

export interface ReceiverState {
	ticket: string
	isReceiving: boolean
	alertDialog: AlertDialogState
}

export interface TicketInputProps {
	ticket: string
	isReceiving: boolean
	previewMetadata: TicketPreviewMetadata | null
	isPreviewLoading: boolean
	onTicketChange: (ticket: string) => void
	onReceive: () => Promise<void>
}
