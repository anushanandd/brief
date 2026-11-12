import { keepPreviousData, queryOptions, useQuery } from '@tanstack/react-query'

import { getReports, type ReportRequest } from '../lib/api'
import type { FinanceSnapshot } from '../lib/schema'
import { snapshotKey } from './use-finance'

/** A request's reports, shared by the page that reads them and the links that load them ahead. */
export const reportsQuery = (data: FinanceSnapshot | undefined, request: ReportRequest) =>
  queryOptions({
    queryKey: ['reports', ...snapshotKey(data), request],
    queryFn: () => getReports(request),
    staleTime: Infinity,
    retry: false,
  })

/**
 * Every report over a window and accounts, and the requested one in full, as Rust computed them
 * from this snapshot. The last reports stay on screen while the next ones load.
 */
export function useReports(
  data: FinanceSnapshot | undefined,
  request: ReportRequest,
  enabled = true,
) {
  return useQuery({
    ...reportsQuery(data, request),
    enabled: enabled && Boolean(data),
    placeholderData: keepPreviousData,
  })
}
