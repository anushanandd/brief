import { useQuery } from '@tanstack/react-query'

import { getPortfolioSectors, isTauri } from '../lib/api'
import type { FinanceSnapshot } from '../lib/schema'
import { holdingsKey } from './use-finance'

/** Sector exposure for the portfolio, shared by every card that asks. */
export function usePortfolioSectors(data: FinanceSnapshot, enabled = true) {
  const native = typeof window !== 'undefined' && isTauri()
  return useQuery({
    queryKey: ['portfolio-sectors', ...holdingsKey(data)],
    queryFn: getPortfolioSectors,
    enabled: native && enabled,
    staleTime: 60 * 60_000,
    retry: false,
  })
}
