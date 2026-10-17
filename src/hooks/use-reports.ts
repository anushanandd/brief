import { keepPreviousData, useQuery } from '@tanstack/react-query'

import { getReports, type ReportRequest } from '../lib/api'
import type { FinanceSnapshot } from '../lib/schema'

/**
 * Every report over a window and accounts, and the requested one in full, as Rust computed them
 * from this snapshot. Insights are recomputed whenever saved data or an edit changes, so their
 * time keys the reports. The last reports stay on screen while the next ones load.
 */
export function useReports(
  data: FinanceSnapshot | undefined,
  request: ReportRequest,
  enabled = true,
) {
  return useQuery({
    queryKey: ['reports', data?.revision, data?.insights?.computedAt, request],
    queryFn: () => getReports(request),
    enabled: enabled && Boolean(data),
    placeholderData: keepPreviousData,
    staleTime: Infinity,
    retry: false,
  })
}
