import { createContext, useContext } from 'react'

import type { FinanceState } from './finance-provider'

export const financeQueryKey = ['finance-snapshot'] as const
export const FinanceContext = createContext<FinanceState | null>(null)

export function useFinance() {
  const finance = useContext(FinanceContext)
  if (!finance) throw new Error('useFinance requires FinanceProvider')
  return finance
}
