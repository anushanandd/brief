import { QueryClient } from '@tanstack/react-query'
import { createMemoryHistory, createRouter } from '@tanstack/react-router'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { seedSnapshot } from './data/fixtures/records'
import { financeQueryKey } from './hooks/use-finance'
import { reportsQuery } from './hooks/use-reports'
import { spendingPeriodQuery } from './hooks/use-spending-period'
import { moneyStripRequests, pageReportRequest, reportDay } from './lib/analytics'
import { getReports, getSpendingPeriod } from './lib/api'
import { spendingPage, spendingPeriodRequest } from './lib/spending'
import { router } from './router'

vi.mock('./lib/api', async (original) => ({
  ...(await original<typeof import('./lib/api')>()),
  getReports: vi.fn(),
  getSpendingPeriod: vi.fn(),
}))

let release: () => void
beforeEach(() => {
  // Rust answers only when the test lets it, so a navigation can be caught waiting.
  const answered = new Promise<void>((resolve) => (release = resolve))
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
  const today = reportDay(data)
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
  const today = reportDay(data)
  const request = pageReportRequest({ range: 'week' }, 'card-credits', today)
  // This calendar year to date: from Jan 1, through today.
  expect(request).toMatchObject({ chart: 'card-credits', from: `${today.slice(0, 4)}-01-01` })
  expect(request.to).toBeUndefined()
  expect(cached(queryClient, reportsQuery(data, request).queryKey)).toBe(true)
})
