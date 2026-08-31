import { useQuery } from '@tanstack/react-query'

import { getFinanceSnapshot } from '../lib/api'

export const financeQueryKey = ['finance-snapshot'] as const

export function useFinance() {
  return useQuery({
    queryKey: financeQueryKey,
    queryFn: getFinanceSnapshot,
    staleTime: Number.POSITIVE_INFINITY,
  })
}
