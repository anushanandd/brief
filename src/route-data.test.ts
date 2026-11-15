import { QueryClient } from '@tanstack/react-query'
import { createMemoryHistory, createRouter } from '@tanstack/react-router'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { seedSnapshot } from './data/fixtures/records'
import { financeQueryKey } from './hooks/use-finance'
import { reportsQuery } from './hooks/use-reports'
import { spendingPeriodQuery } from './hooks/use-spending-period'
import { moneyStripRequests, pageReportRequest } from './lib/analytics'
import {
  getEarningsCalendar,
  getInvestmentSummary,
  getMarketNews,
  getReports,
  getSpendingPeriod,
  isTauri,
} from './lib/api'
import { snapshotToday } from './lib/dates'
import { spendingPage, spendingPeriodRequest } from './lib/spending'
import { earningsCalendarQuery, holdingNewsQuery } from './route-data'
import { router } from './router'

vi.mock('./lib/api', async (original) => ({
  ...(await original<typeof import('./lib/api')>()),
  isTauri: vi.fn(() => false),
  getReports: vi.fn(),
  getSpendingPeriod: vi.fn(),
  getMarketNews: vi.fn(),
  getEarningsCalendar: vi.fn(),
  getInvestmentSummary: vi.fn(),
}))

let release: () => void
let answered: Promise<void>
beforeEach(() => {
  // Rust answers only when the test lets it, so a navigation can be caught waiting.
  answered = new Promise<void>((resolve) => (release = resolve))
  vi.mocked(getReports).mockImplementation(() => answered.then(() => null))
  vi.mocked(getSpendingPeriod).mockImplementation(() => answered.then(() => null))
})
afterEach(() => vi.clearAllMocks())

function app(path: string) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const data = seedSnapshot()
  queryClient.setQueryData(financeQueryKey, data)
  const testRouter = createRouter({
    routeTree: router.routeTree,
    history: createMemoryHistory({ initialEntries: [path] }),
    context: { queryClient },
  })
  return { queryClient, data, testRouter }
}

const cached = (client: QueryClient, key: readonly unknown[]) =>
  client.getQueryState(key)?.status === 'success'

it('opens a report only once its data and its strip’s figures are in', async () => {
  const { queryClient, data, testRouter } = app('/analytics?chart=cash-flow&range=month')
  let opened = false
  const loading = testRouter.load().then(() => (opened = true))
  await vi.waitFor(() => expect(getReports).toHaveBeenCalledTimes(3))
  expect(opened).toBe(false)
  release()
  await loading
  const today = snapshotToday(data)
  const search = { chart: 'cash-flow', range: 'month' } as const
  for (const request of [
    pageReportRequest(search, 'cash-flow', today),
    ...moneyStripRequests(search, today),
  ])
    expect(cached(queryClient, reportsQuery(data, request).queryKey)).toBe(true)
})

it('opens Spending only once its period is in', async () => {
  const { queryClient, data, testRouter } = app('/spending?range=month')
  let opened = false
  const loading = testRouter.load().then(() => (opened = true))
  await vi.waitFor(() => expect(getSpendingPeriod).toHaveBeenCalled())
  expect(opened).toBe(false)
  release()
  await loading
  const page = spendingPage(data, { range: 'month' })
  expect(
    cached(queryClient, spendingPeriodQuery(data, spendingPeriodRequest(page, 0)).queryKey),
  ).toBe(true)
})

it('opens a holding once its saved news and earnings date are in, with its summary started', async () => {
  vi.mocked(isTauri).mockReturnValue(true)
  vi.mocked(getMarketNews).mockImplementation(() =>
    answered.then(() => ({
      articles: [],
      savedAt: null,
      warning: null,
      canRefresh: false,
      company: null,
      filings: [],
      filingsSavedAt: null,
      filingsWarning: null,
    })),
  )
  vi.mocked(getEarningsCalendar).mockImplementation(() => answered.then(() => []))
  // A summary waiting on market data never holds the page.
  vi.mocked(getInvestmentSummary).mockImplementation(() => new Promise(() => undefined))
  const { queryClient, testRouter } = app('/holdings/MSFT')
  queryClient.setQueryData(['integration-status'], { alphaVantage: true })
  let opened = false
  const loading = testRouter.load().then(() => (opened = true))
  await vi.waitFor(() => expect(getEarningsCalendar).toHaveBeenCalledWith(['MSFT']))
  expect(getMarketNews).toHaveBeenCalledWith(['MSFT'])
  expect(getInvestmentSummary).toHaveBeenCalledWith(expect.any(String), 'MSFT')
  expect(opened).toBe(false)
  release()
  await loading
  expect(cached(queryClient, holdingNewsQuery('MSFT').queryKey)).toBe(true)
  expect(cached(queryClient, earningsCalendarQuery(['MSFT']).queryKey)).toBe(true)
  vi.mocked(isTauri).mockReturnValue(false)
})

it('leaves a failed read to the page instead of failing the navigation', async () => {
  vi.mocked(getReports).mockRejectedValue(new Error('Rust is unavailable'))
  const { testRouter } = app('/analytics?chart=income')
  await testRouter.load()
  expect(testRouter.state.matches.at(-1)?.status).toBe('success')
})

it('opens Benefits on its own year whatever period the others share', async () => {
  const { queryClient, data, testRouter } = app('/analytics?chart=card-credits&range=week')
  release()
  await testRouter.load()
  const today = snapshotToday(data)
  const request = pageReportRequest({ range: 'week' }, 'card-credits', today)
  // This calendar year to date: from Jan 1, through today.
  expect(request).toMatchObject({ chart: 'card-credits', from: `${today.slice(0, 4)}-01-01` })
  expect(request.to).toBeUndefined()
  expect(cached(queryClient, reportsQuery(data, request).queryKey)).toBe(true)
})
