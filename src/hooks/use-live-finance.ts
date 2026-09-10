import { createContext, useContext } from 'react'

import type { LiveFinanceState } from './live-market-provider'

export const LiveFinanceContext = createContext<LiveFinanceState | null>(null)

export function useLiveFinance() {
  const finance = useContext(LiveFinanceContext)
  if (!finance) throw new Error('useLiveFinance requires LiveMarketProvider')
  return finance
}
