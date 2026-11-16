// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import type { LivelineProps } from 'liveline'
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
let quote: { kind: 'quote'; price: number; time: number } | undefined
vi.mock('../hooks/use-holding-prices', () => ({
  useHoldingPrices: (symbol: string, range: number) => {
    if (!histories.has(range)) histories.set(range, { ...daily, range })
    return {
      symbol,
      range,
      history: histories.get(range),
      quote,
      bars: [],
      stale: false,
      visible: true,
      now,
    }
  },
}))

vi.mock('../lib/api', async (original) => ({
  ...(await original<typeof import('../lib/api')>()),
  getPeriodAsOf: vi.fn(async (end: string) => ({
    end,
    chartChanges: [],
    periods: [],
    flows: [],
    changes: [],
    recap: [],
  })),
}))
vi.mock('../hooks/use-finance', async (original) => ({
  ...(await original<typeof import('../hooks/use-finance')>()),
  useFinance: () => ({
    data: { insights: { today: '2026-09-30', periods: [] } },
    marketSymbols: ['TEST'],
    liveMarketEnabled: true,
  }),
}))
const drawn: LivelineProps[] = []
vi.mock('liveline', async (original) => {
  const actual = await original<typeof import('liveline')>()
  return {
    ...actual,
    Liveline: (props: LivelineProps) => {
      drawn.push(props)
      return <actual.Liveline {...props} />
    },
  }
})
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
  quote = undefined
  drawn.length = 0
})

const holding = (
  <QueryClientProvider client={new QueryClient()}>
    <HoldingChart
      holdings={[{ ticker: 'TEST', name: 'Test', price: 100 }]}
      ticker="TEST"
      range="quarter"
    />
  </QueryClientProvider>
)

it('badges daily candles with a trade newer than the last one, as the price above reads', async () => {
  // Before the open: the last candle is yesterday's close.
  quote = { kind: 'quote', price: 555.5, time: now - 60 }
  const host = document.body.appendChild(document.createElement('div'))
  const root = createRoot(host)
  await act(async () => root.render(holding))
  const lastClose = drawn.at(-1)!.candles!.at(-1)!.close
  expect(lastClose).not.toBe(555.5)
  expect(drawn.at(-1)!.badgeValue).toBe(555.5)
  expect(host.querySelector('.home-balance-value')!.textContent).toBe('555.5')
  // Scrolled back, the day's candles badge themselves.
  await act(async () => setGraphEnd('2026-09-01'))
  expect(drawn.at(-1)!.badgeValue).toBeUndefined()
  await act(async () => root.unmount())
  setGraphEnd(undefined)
})

it('badges daily candles on their last close without a newer trade', async () => {
  quote = { kind: 'quote', price: 555.5, time: now - 10 * 86_400 }
  const host = document.body.appendChild(document.createElement('div'))
  const root = createRoot(host)
  await act(async () => root.render(holding))
  expect(drawn.at(-1)!.badgeValue).toBeUndefined()
  await act(async () => root.unmount())
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
  const renders = drawn.length
  const price = () => host.querySelector('.home-balance-value')!.textContent
  const change = () => host.querySelector('.chart-summary-row')!.textContent
  const prices = new Set<string>()
  const changes = new Set<string>()
  for (let index = 0; index < 70; index += 1) {
    await act(async () =>
      chart.dispatchEvent(new WheelEvent('wheel', { deltaY: 10, bubbles: true, cancelable: true })),
    )
    await act(async () => vi.advanceTimersToNextFrame())
    prices.add(price())
    changes.add(change())
  }
  expect(pageCommits).toBe(0)
  // The chart renders once, as the scroll starts; its readouts follow every session it crosses.
  expect(drawn.length - renders).toBeLessThanOrEqual(1)
  expect(prices.size).toBeGreaterThanOrEqual(20)
  expect(changes.size).toBeGreaterThanOrEqual(20)
  expect(price()).toMatch(/^\$\d+\.\d\d$/)
  // Eased onto the session, the readout holds until the day's figures arrive.
  await act(async () => vi.advanceTimersByTime(160 + 250))
  const settling = Number(price().replace('$', ''))
  await act(async () => vi.advanceTimersByTime(1_000))
  console.info(`30 days of a candle chart on M: ${pageCommits} page commits, ${commits} commits`)
  // 21 sessions back, onto one of them.
  expect(weekdays).toContain(sessionGraphEnd())
  expect(sessionGraphEnd()! >= '2026-08-28' && sessionGraphEnd()! <= '2026-09-01').toBe(true)
  // The animated price takes over at the close the readout settled on.
  expect(price()).toBe(String(settling))
  expect(pageCommits).toBeLessThanOrEqual(2)
  await act(async () => root.unmount())
  setGraphEnd(undefined)
})
