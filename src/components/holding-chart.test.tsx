// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, Profiler } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from 'vitest'

import { addDays } from '../lib/dates'
import {
  resetSessionGraphWindow,
  sessionGraphEnd,
  setGraphEnd,
  useGraphEnd,
} from '../lib/graph-preferences'
import { holdingSessions } from '../lib/holding-prices'
import type { HoldingPriceHistory } from '../lib/schema'
import { HoldingChart } from './holding-chart'

const now = Date.parse('2026-09-30T18:00:00Z') / 1_000
const weekdays = Array.from({ length: 640 }, (_, index) => addDays('2025-01-01', index)).filter(
  (date) => date <= '2026-09-30' && new Date(`${date}T12:00:00Z`).getUTCDay() % 6,
)
const calendar: HoldingPriceHistory = {
  symbol: 'TEST',
  range: 0,
  resolution: 86_400,
  start: Date.parse('2025-01-01T00:00:00Z') / 1_000,
  end: now,
  fetchedAt: now * 1_000,
  bars: [],
  feeds: ['sip'],
  delayMinutes: 0,
  adjustment: 'split',
  cached: false,
  calendar: weekdays.map((date) => ({ date, open: 34_200, close: 57_600 })),
}
const daily = {
  ...calendar,
  bars: holdingSessions(calendar)
    .filter((session) => session.kind === 'regular' && session.end < now)
    .map((session, index) => ({
      time: session.start,
      endTime: session.end,
      open: 100 + index,
      high: 102 + index,
      low: 99 + index,
      close: 101 + index,
      volume: 10,
      trades: 2,
      feed: 'sip' as const,
    })),
}
const histories = new Map<number, HoldingPriceHistory>()
vi.mock('../hooks/use-holding-prices', () => ({
  useHoldingPrices: (symbol: string, range: number) => {
    if (!histories.has(range)) histories.set(range, { ...daily, range })
    return {
      symbol,
      range,
      history: histories.get(range),
      bars: [],
      stale: false,
      visible: true,
      now,
    }
  },
}))

vi.mock('../hooks/use-finance', async (original) => ({
  ...(await original<typeof import('../hooks/use-finance')>()),
  useFinance: () => ({
    data: { insights: { today: '2026-09-30', periods: [] } },
    marketSymbols: ['TEST'],
    liveMarketEnabled: true,
  }),
}))
// jsdom can't run an animated figure (NumberFlow) once it updates.
vi.mock('@number-flow/react', () => ({
  default: ({ value }: { value: number }) => <span>{value}</span>,
}))

beforeEach(() => {
  vi.useFakeTimers({
    toFake: ['Date', 'setTimeout', 'clearTimeout', 'requestAnimationFrame', 'cancelAnimationFrame'],
  })
  vi.setSystemTime(now * 1_000)
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  )
  vi.stubGlobal('matchMedia', () => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }))
  const original = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, 'getContext')!
  Object.defineProperty(HTMLCanvasElement.prototype, 'getContext', {
    configurable: true,
    value: () => null,
  })
  onTestFinished(() => {
    Object.defineProperty(HTMLCanvasElement.prototype, 'getContext', original)
  })
  resetSessionGraphWindow()
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  resetSessionGraphWindow()
  document.body.replaceChildren()
})

it('slides a candle chart a month back without a page commit until it settles', async () => {
  let pageRendered = false
  let pageCommits = 0
  let commits = 0
  function Page() {
    pageRendered = true
    useGraphEnd()
    return (
      <HoldingChart
        holdings={[{ ticker: 'TEST', name: 'Test', price: 100 }]}
        ticker="TEST"
        range="month"
      />
    )
  }
  const host = document.body.appendChild(document.createElement('div'))
  const root = createRoot(host)
  await act(async () =>
    root.render(
      <QueryClientProvider client={new QueryClient()}>
        <Profiler
          id="page"
          onRender={() => {
            commits += 1
            if (pageRendered) pageCommits += 1
            pageRendered = false
          }}
        >
          <Page />
        </Profiler>
      </QueryClientProvider>,
    ),
  )
  const chart = host.querySelector('.brokerage-chart-viewport')!
  // 700 pixels show the month's 21 sessions.
  Object.defineProperty(chart, 'clientWidth', { value: 700 })
  // What waits to show, like a loading message, has shown.
  await act(async () => vi.advanceTimersByTime(2_000))
  pageCommits = 0
  commits = 0
  for (let index = 0; index < 70; index += 1) {
    await act(async () =>
      chart.dispatchEvent(new WheelEvent('wheel', { deltaY: 10, bubbles: true, cancelable: true })),
    )
    await act(async () => vi.advanceTimersToNextFrame())
  }
  expect(pageCommits).toBe(0)
  // The chart's own, as the scroll starts.
  expect(commits).toBeLessThanOrEqual(2)
  await act(async () => vi.advanceTimersByTime(1_000))
  console.info(`30 days of a candle chart on M: ${pageCommits} page commits, ${commits} commits`)
  // 21 sessions back, onto one of them.
  expect(weekdays).toContain(sessionGraphEnd())
  expect(sessionGraphEnd()! >= '2026-08-28' && sessionGraphEnd()! <= '2026-09-01').toBe(true)
  expect(pageCommits).toBeLessThanOrEqual(2)
  await act(async () => root.unmount())
  setGraphEnd(undefined)
})
