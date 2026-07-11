import { create } from 'zustand'

export type TransferTab = 'send' | 'receive'

type TransferTabState = {
	requestedTab: TransferTab | null
	requestTab: (tab: TransferTab) => void
	clearRequestedTab: () => void
}

export const useTransferTabStore = create<TransferTabState>((set) => ({
	requestedTab: null,
	requestTab: (tab) => set({ requestedTab: tab }),
	clearRequestedTab: () => set({ requestedTab: null }),
}))
