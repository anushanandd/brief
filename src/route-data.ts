/**
 * What a page needs before it opens. Each route's loader fetches its page's queries, built by
 * the same functions the page renders from, and the router holds the current page until they
 * arrive, so navigation never shows a loading screen. Before the first snapshot there is
 * nothing to fetch.
 */
import type { QueryClient } from '@tanstack/react-query'

import { financeQueryKey, snapshotKey } from './hooks/use-finance'
import { reportsQuery } from './hooks/use-reports'
import { spendingPeriodQuery } from './hooks/use-spending-period'
import {
  analyticsCharts,
  moneyStripRequests,
  pageReportRequest,
  reportDay,
  resolvedAnalyticsSearch,
  type AnalyticsSearch,
} from './lib/analytics'
import { getDataHealth, type ReportRequest } from './lib/api'
import { chosenGraphWindow } from './lib/graph-preferences'
import type { FinanceSnapshot } from './lib/schema'
import {
  reportRangeForPeriod,
  spendingPage,
  spendingPeriodRequest,
  type SpendingSearch,
} from './lib/spending'

export type RouterContext = { queryClient: QueryClient }

/** Data health's query, shared by its page and its route. */
export const dataHealthQuery = (data: FinanceSnapshot | undefined) => ({
  queryKey: ['data-health', ...snapshotKey(data)],
  queryFn: getDataHealth,
})

const snapshot = (client: QueryClient) => client.getQueryData<FinanceSnapshot>(financeQueryKey)

/**
 * Waits for queries the way the page will read them. A failure is left to the page, which
 * shows it where it belongs, rather than replacing the page with the router's error.
 */
const settle = (work: Array<Promise<unknown>>) => Promise.allSettled(work).then(() => undefined)

const reports = (client: QueryClient, data: FinanceSnapshot, requests: readonly ReportRequest[]) =>
  requests.map((request) => client.ensureQueryData(reportsQuery(data, request)))

/** A report, with Money's strip above it when the report is Money's. */
export function loadAnalytics({ queryClient }: RouterContext, search: AnalyticsSearch) {
  const data = snapshot(queryClient)
  if (!data) return undefined
  const resolved = resolvedAnalyticsSearch(search, chosenGraphWindow())
  const chart = resolved.chart ?? analyticsCharts[0].id
  const today = reportDay(data)
  const money = analyticsCharts.find(({ id }) => id === chart)?.group === 'money'
  return settle(
    reports(queryClient, data, [
      pageReportRequest(resolved, chart, today),
      ...(money ? moneyStripRequests(resolved, today) : []),
    ]),
  )
}

/** Spending's latest period, and Money's strip over it. */
export function loadSpending({ queryClient }: RouterContext, search: SpendingSearch) {
  const data = snapshot(queryClient)
  if (!data?.insights) return undefined
  const page = spendingPage(data, search)
  return settle([
    queryClient.ensureQueryData(spendingPeriodQuery(data, spendingPeriodRequest(page, 0))),
    ...reports(
      queryClient,
      data,
      moneyStripRequests({ range: reportRangeForPeriod(page.period) }, reportDay(data)),
    ),
  ])
}

export function loadDataHealth({ queryClient }: RouterContext) {
  const data = snapshot(queryClient)
  if (!data) return undefined
  return settle([queryClient.ensureQueryData(dataHealthQuery(data))])
}
