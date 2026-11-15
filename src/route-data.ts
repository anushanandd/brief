/**
 * What a page needs before it opens. Each route's loader fetches its page's queries, built by
 * the same functions the page renders from, and the router holds the current page until they
 * arrive, so navigation never shows a loading screen. Before the first snapshot there is
 * nothing to fetch.
 */
import { queryOptions, type QueryClient } from '@tanstack/react-query'

import { investmentSummaryQuery } from './components/investment-summary-card'
import { financeQueryKey, snapshotKey } from './hooks/use-finance'
import { reportsQuery } from './hooks/use-reports'
import { spendingPeriodQuery } from './hooks/use-spending-period'
import {
  analyticsCharts,
  moneyStripRequests,
  pageReportRequest,
  resolvedAnalyticsSearch,
  type AnalyticsSearch,
} from './lib/analytics'
import {
  getDataHealth,
  getEarningsCalendar,
  getMarketNews,
  isTauri,
  type ReportRequest,
} from './lib/api'
import { snapshotToday } from './lib/dates'
import { chartPeriod, chosenGraphWindow, sessionGraphWindow } from './lib/graph-preferences'
import { summaryStart } from './lib/investment-summary'
import type { FinanceSnapshot, IntegrationStatus } from './lib/schema'
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

/** A holding's saved stories and filings, shared by its chart, its News card, and its route. */
export const holdingNewsQuery = (ticker: string) =>
  queryOptions({
    queryKey: ['holding-news', 'alpaca', ticker],
    queryFn: () => getMarketNews([ticker]),
    staleTime: Infinity,
    retry: false,
  })

/** The saved next earnings dates, shared by the position card and the holding's route. */
export const earningsCalendarQuery = (symbols: string[]) =>
  queryOptions({
    queryKey: ['earnings-calendar', symbols],
    queryFn: () => getEarningsCalendar(symbols),
    staleTime: 6 * 60 * 60 * 1000,
    retry: false,
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
  const today = snapshotToday(data)
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
      moneyStripRequests({ range: reportRangeForPeriod(page.period) }, snapshotToday(data)),
    ),
  ])
}

/**
 * A holding's saved news and next earnings date. Its summary may wait on market data, so it
 * starts here without holding the page.
 */
export function loadSecurity({ queryClient }: RouterContext, ticker: string) {
  const data = snapshot(queryClient)
  if (!data || !isTauri() || !data.holdings.some((holding) => holding.ticker === ticker))
    return undefined
  const start = summaryStart(data, chartPeriod(sessionGraphWindow()))
  if (start) void queryClient.prefetchQuery(investmentSummaryQuery(start, ticker, data))
  const earnings = queryClient.getQueryData<IntegrationStatus>(['integration-status'])?.alphaVantage
  return settle([
    queryClient.ensureQueryData(holdingNewsQuery(ticker)),
    ...(earnings ? [queryClient.ensureQueryData(earningsCalendarQuery([ticker]))] : []),
  ])
}

export function loadDataHealth({ queryClient }: RouterContext) {
  const data = snapshot(queryClient)
  if (!data) return undefined
  return settle([queryClient.ensureQueryData(dataHealthQuery(data))])
}
