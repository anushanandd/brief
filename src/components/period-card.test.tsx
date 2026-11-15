// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import {
  createMemoryHistory,
  createRootRoute,
  createRouter,
  RouterProvider,
} from '@tanstack/react-router'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, expect, it, vi } from 'vitest'

import { insights } from '../data/fixtures/insights'
import { seedSnapshot } from '../data/fixtures/records'
import { getReports, type ReportRequest } from '../lib/api'
import { setGraphEnd } from '../lib/graph-preferences'
import { PeriodCard } from './investments-summary'

// Each request waits until the test answers it.
const answers = vi.hoisted(() => [] as Array<() => void>)
vi.mock('../lib/api', async (original) => ({
  ...(await original<typeof import('../lib/api')>()),
  getReports: vi.fn(
    () => new Promise((resolve) => answers.push(() => resolve({ summaries: [], report: null }))),
  ),
}))

afterEach(() => {
  setGraphEnd(undefined)
  document.body.replaceChildren()
})

const settle = () => act(() => new Promise((resolve) => setTimeout(resolve, 10)))
// The answer commits in one act, and the card's next request in the one after.
const answer = async () => {
  await act(async () => answers.shift()?.())
  await settle()
  await settle()
}

const askedDays = () =>
  vi.mocked(getReports).mock.calls.map(([request]: [ReportRequest]) => request.to)

it('asks for one scrolled day at a time, the latest when the last answer arrives', async () => {
  const data = seedSnapshot()
  data.insights = insights({ today: '2026-09-16' })
  const client = new QueryClient()
  const router = createRouter({
    routeTree: createRootRoute({
      component: () => (
        <QueryClientProvider client={client}>
          <PeriodCard data={data} range="month" />
        </QueryClientProvider>
      ),
    }),
    history: createMemoryHistory({ initialEntries: ['/'] }),
  })
  await router.load()
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () => root.render(<RouterProvider router={router} />))
  expect(askedDays()).toEqual(['2026-09-16'])

  // Three days scrolled while today's answer is out.
  await act(async () => setGraphEnd('2026-09-15'))
  await act(async () => setGraphEnd('2026-09-14'))
  await act(async () => setGraphEnd('2026-09-13'))
  expect(askedDays()).toEqual(['2026-09-16'])
  await answer()
  expect(askedDays()).toEqual(['2026-09-16', '2026-09-13'])
  await answer()
  expect(askedDays()).toHaveLength(2)
  await act(async () => root.unmount())
  client.clear()
})
