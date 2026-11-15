// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, expect, it, vi } from 'vitest'

import { startHoldingChart } from '../lib/api'
import { holdingChartEventSchema, type HoldingPriceHistory } from '../lib/schema'
import { useHoldingPrices } from './use-holding-prices'

const listeners = vi.hoisted(() => new Set<(event: { payload: unknown }) => void>())
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (_name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.add(handler)
    return () => listeners.delete(handler)
  }),
}))
vi.mock('../lib/api', () => ({
  isTauri: () => true,
  startHoldingChart: vi.fn(async () => undefined),
  stopHoldingChart: vi.fn(async () => undefined),
}))
afterEach(() => {
  vi.clearAllMocks()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

const fetchedAt = Date.parse('2026-09-16T21:00:00Z')
const history = (symbol: string, range: number): HoldingPriceHistory => ({
  symbol,
  range,
  resolution: 60,
  start: fetchedAt / 1000 - 3600,
  end: fetchedAt / 1000,
  fetchedAt,
  bars: [0, 60].map((offset) => ({
    time: fetchedAt / 1000 - 600 + offset,
    endTime: fetchedAt / 1000 - 540 + offset,
    open: 100,
    high: 100,
    low: 100,
    close: 100,
    volume: 10,
    trades: 2,
    feed: 'sip',
  })),
  feeds: ['sip'],
  delayMinutes: 0,
  adjustment: 'split',
  cached: false,
})
const emit = (symbol: string, range: number) => {
  const requestId = vi.mocked(startHoldingChart).mock.calls.at(-1)?.[2]
  const payload = {
    kind: 'history',
    symbol,
    requestId,
    history: history(symbol, range),
    startedAt: 0,
  }
  for (const listener of listeners) listener({ payload })
}

function mount() {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  const client = new QueryClient()
  const root = createRoot(document.createElement('div'))
  let result!: ReturnType<typeof useHoldingPrices>
  function Probe({ symbol, range }: { symbol: string; range: number }) {
    result = useHoldingPrices(symbol, range, true, ['AAA', 'BBB'])
    return null
  }
  const render = (symbol: string, range: number) =>
    act(async () =>
      root.render(
        <QueryClientProvider client={client}>
          <Probe symbol={symbol} range={range} />
        </QueryClientProvider>,
      ),
    )
  return { render, current: () => result, unmount: () => act(async () => root.unmount()) }
}

it('keeps a warmed neighbor unread until its chart opens, then shows it as saved', async () => {
  const chart = mount()
  await chart.render('AAA', 86400)
  const parse = vi.spyOn(holdingChartEventSchema, 'safeParse')
  await act(async () => emit('BBB', 86400))
  expect(parse).not.toHaveBeenCalled()
  await chart.render('BBB', 86400)
  expect(chart.current().history?.symbol).toBe('BBB')
  expect(chart.current().history?.cached).toBe(true)
  await chart.unmount()
})

it('keeps the previous range on screen, marked stale, until the new range arrives', async () => {
  const chart = mount()
  await chart.render('AAA', 86400)
  await act(async () => emit('AAA', 86400))
  expect(chart.current().stale).toBe(false)
  await chart.render('AAA', 0)
  expect(chart.current().history?.range).toBe(86400)
  expect(chart.current().stale).toBe(true)
  await act(async () => emit('AAA', 0))
  expect(chart.current().history?.range).toBe(0)
  expect(chart.current().stale).toBe(false)
  // Another ticker never inherits the last one's chart.
  await chart.render('BBB', 0)
  expect(chart.current().history).toBeUndefined()
  expect(chart.current().stale).toBe(false)
  await chart.unmount()
})
