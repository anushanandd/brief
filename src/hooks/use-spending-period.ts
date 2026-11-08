import { keepPreviousData, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'

import { getSpendingPeriod, type SpendingPeriodRequest } from '../lib/api'
import type { FinanceSnapshot } from '../lib/schema'
import { allSpendingAccounts, spendingPeriodKind } from '../lib/spending'

/**
 * Insights are recomputed whenever saved data or an edit changes, so their time keys a period
 * along with the request.
 */
const periodKey = (data: FinanceSnapshot | undefined, request: SpendingPeriodRequest) =>
  ['spending-period', data?.revision, data?.insights?.computedAt, request] as const

/**
 * One Spending period in full, as Rust computed it from this snapshot. The last period stays
 * on screen while the next one loads.
 */
export function useSpendingPeriod(
  data: FinanceSnapshot | undefined,
  request: SpendingPeriodRequest,
) {
  return useQuery({
    queryKey: periodKey(data, request),
    queryFn: () => getSpendingPeriod(request),
    enabled: Boolean(data?.insights),
    placeholderData: keepPreviousData,
    staleTime: Infinity,
    retry: false,
  })
}

/** The period Money opens on without a search: the user's own spending this month, everywhere. */
export function openingSpendingRequest(): SpendingPeriodRequest {
  return { scope: allSpendingAccounts, owner: 'me', kind: spendingPeriodKind(1), offset: 0 }
}

/** Reads the period Money opens on as each snapshot arrives, so Money opens without waiting. */
export function usePrefetchOpeningSpending(data: FinanceSnapshot | undefined) {
  const client = useQueryClient()
  useEffect(() => {
    if (!data?.insights) return
    const request = openingSpendingRequest()
    void client.prefetchQuery({
      queryKey: periodKey(data, request),
      queryFn: () => getSpendingPeriod(request),
      staleTime: Infinity,
      retry: false,
    })
  }, [client, data])
}
