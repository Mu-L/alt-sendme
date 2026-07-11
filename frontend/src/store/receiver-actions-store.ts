import { create } from 'zustand'
import type { PairedInvitePayload } from '@/lib/pairing-api'

type ReceiverActionsState = {
	acceptPairedInvite: ((invite: PairedInvitePayload) => Promise<void>) | null
	registerAcceptPairedInvite: (
		handler: ((invite: PairedInvitePayload) => Promise<void>) | null
	) => void
}

export const useReceiverActionsStore = create<ReceiverActionsState>((set) => ({
	acceptPairedInvite: null,
	registerAcceptPairedInvite: (handler) => set({ acceptPairedInvite: handler }),
}))
