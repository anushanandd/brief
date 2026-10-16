import { useQuery } from '@tanstack/react-query'

import { getPortfolioSectors, isTauri } from '../lib/api'
import type { FinanceSnapshot } from '../lib/schema'

/** Sector exposure for the portfolio, shared by every card that asks. */
export function usePortfolioSectors(data: FinanceSnapshot, enabled = true) {
  const native = typeof window !== 'undefined' && isTauri()
  return useQuery({
    queryKey: ['portfolio-sectors', data.revision],
    queryFn: getPortfolioSectors,
    enabled: native && enabled,
    staleTime: 60 * 60_000,
    retry: false,
  })
}
