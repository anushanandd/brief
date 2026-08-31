import { create } from 'zustand'

type UiState = {
  commandOpen: boolean
  activeAccountId: string
  setCommandOpen: (open: boolean) => void
  setActiveAccountId: (id: string) => void
}

export const useUiStore = create<UiState>((set) => ({
  commandOpen: false,
  activeAccountId: 'all',
  setCommandOpen: (commandOpen) => set({ commandOpen }),
  setActiveAccountId: (activeAccountId) => set({ activeAccountId }),
}))
