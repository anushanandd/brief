import { keepPreviousData, queryOptions, useQuery } from '@tanstack/react-query'

import { getSpendingPeriod, type SpendingPeriodRequest } from '../lib/api'
import type { FinanceSnapshot } from '../lib/schema'
import { snapshotKey } from './use-finance'

/** A Spending period's query, shared by the page that reads it and the route that loads it. */
export const spendingPeriodQuery = (
  data: FinanceSnapshot | undefined,
  request: SpendingPeriodRequest,
) =>
  queryOptions({
    queryKey: ['spending-period', ...snapshotKey(data), request],
    queryFn: () => getSpendingPeriod(request),
    staleTime: Infinity,
    retry: false,
  })

/**
 * One Spending period in full, as Rust computed it from this snapshot. The last period stays
 * on screen while the next one loads.
 */
export function useSpendingPeriod(
  data: FinanceSnapshot | undefined,
  request: SpendingPeriodRequest,
) {
  return useQuery({
    ...spendingPeriodQuery(data, request),
    enabled: Boolean(data?.insights),
    placeholderData: keepPreviousData,
  })
}
