import { keepPreviousData, useQuery } from '@tanstack/react-query'

import { getSpendingPeriod, type SpendingPeriodRequest } from '../lib/api'
import type { FinanceSnapshot } from '../lib/schema'

/**
 * One Spending period in full, as Rust computed it from this snapshot. Insights are recomputed
 * whenever saved data or an edit changes, so their time keys the period. The last period stays
 * on screen while the next one loads.
 */
export function useSpendingPeriod(
  data: FinanceSnapshot | undefined,
  request: SpendingPeriodRequest,
) {
  return useQuery({
    queryKey: ['spending-period', data?.revision, data?.insights?.computedAt, request],
    queryFn: () => getSpendingPeriod(request),
    enabled: Boolean(data?.insights),
    placeholderData: keepPreviousData,
    staleTime: Infinity,
    retry: false,
  })
}
