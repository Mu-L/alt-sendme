import { create } from 'zustand'
import type { PairedInvitePayload } from '@/lib/pairing-api'

type ReceiverActionsState = {
	acceptPairedInvite: ((invite: PairedInvitePayload) => Promise<void>) | null
	browseSaveFolder: (() => Promise<void>) | null
	savePath: string
	registerAcceptPairedInvite: (
		handler: ((invite: PairedInvitePayload) => Promise<void>) | null
	) => void
	registerBrowseSaveFolder: (handler: (() => Promise<void>) | null) => void
	setReceiverSavePath: (path: string) => void
}

export const useReceiverActionsStore = create<ReceiverActionsState>((set) => ({
	acceptPairedInvite: null,
	browseSaveFolder: null,
	savePath: '',
	registerAcceptPairedInvite: (handler) => set({ acceptPairedInvite: handler }),
	registerBrowseSaveFolder: (handler) => set({ browseSaveFolder: handler }),
	setReceiverSavePath: (path) => set({ savePath: path }),
}))
