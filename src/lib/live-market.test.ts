import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { emptyRollup, insights } from '../data/fixtures/insights'
import { seedSnapshot } from '../data/fixtures/records'
import { startMarketStream, stopMarketStream } from './api'
import {
  applyLiveProjection,
  forSnapshot,
  liveMarketSeries,
  subscribeLiveMarket,
  withMarketFrame,
  withMarketHistory,
  withMarketWeek,
  type LiveMarketState,
} from './live-market'
import { marketSnapshotsSchema, type ChartChanges, type MarketProjection } from './schema'
import { present } from './test-support'

vi.mock('./api', () => ({
  startMarketStream: vi.fn(),
  stopMarketStream: vi.fn(),
}))
const start = vi.mocked(startMarketStream)
const stop = vi.mocked(stopMarketStream)
const startCall = (index: number) => present(start.mock.calls[index], `start call ${index}`)
beforeEach(() => {
  vi.useFakeTimers()
  vi.stubGlobal('window', new EventTarget())
  start.mockReset().mockResolvedValue(undefined)
  stop.mockReset().mockResolvedValue(undefined)
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

it('rejects stale revisions and timestamps and retains immutable history references', () => {
  const saved = { ...seedSnapshot(), revision: 4 }
  const projection: MarketProjection = {
    rollup: emptyRollup(),
    ...saved,
    netWorth: 123,
    brokeragePerformance: saved.brokeragePerformance.map(({ accountId, currentValue }) => ({
      accountId,
      currentValue: (currentValue ?? 0) + 1,
    })),
  }
  expect(applyLiveProjection(saved, { ...projection, revision: 3 })).toBe(saved)
  expect(applyLiveProjection(saved, { ...projection, updatedAt: 'old' })).toBe(saved)
  const live = applyLiveProjection(saved, projection)
  expect(live.netWorth).toBe(123)
  expect(live.transactions).toBe(saved.transactions)
  expect(live.netWorthHistory).toBe(saved.netWorthHistory)
  saved.brokeragePerformance.forEach((account, index) =>
    expect(live.brokeragePerformance[index]?.points).toBe(account.points),
  )
  // The account totals are Rust's at the live values, never the saved ones.
  const totals = emptyRollup({ netWorth: 120 })
  const withInsights = { ...saved, insights: insights() }
  expect(
    applyLiveProjection(withInsights, { ...projection, rollup: totals }).insights?.rollup,
  ).toBe(totals)
  expect(saved.netWorth).not.toBe(123)
})

it('retains previous close and five-minute portfolio projections instead of only endpoints', () => {
  let series: Record<string, Array<{ time: number; value: number }>> = {}
  for (const [time, value] of [
    [1_000, 100],
    [1_200, 101],
    [1_260, 102],
    [1_320, 103],
    [2_400, 104],
  ] as const) {
    series = liveMarketSeries(series, {
      ...closedMarket(),
      chartPoint: { brokerage: { time, value } },
    })
  }
  expect(series.brokerage).toEqual([
    { time: 960, value: 100 },
    { time: 1_320, value: 103 },
    { time: 2_400, value: 104 },
  ])
})

it('atomically replaces every current valuation while retaining committed evidence', () => {
  const saved = { ...seedSnapshot(), revision: 4 }
  const accounts = saved.accounts.map((account, index) => ({
    ...account,
    value: account.value == null ? null : account.value + index + 10,
    cashValue: 25 + index,
    investedValue: 75 + index,
  }))
  const holdings = saved.holdings.map((holding, index) => ({
    ...holding,
    price: holding.price == null ? null : holding.price + index + 1,
    value: holding.value == null ? null : holding.value + index + 20,
  }))
  const projection: MarketProjection = {
    rollup: emptyRollup(),
    revision: 4,
    updatedAt: saved.updatedAt,
    netWorth: saved.netWorth + 100,
    netWorthIncomplete: saved.netWorthIncomplete,
    netWorthProvisional: saved.netWorthProvisional,
    accounts,
    holdings,
    brokeragePerformance: saved.brokeragePerformance.map(({ accountId, currentValue }, index) => ({
      accountId,
      currentValue: currentValue + index + 30,
    })),
  }

  const live = applyLiveProjection(saved, projection)
  expect(live.netWorth).toBe(projection.netWorth)
  expect(live.accounts).toEqual(accounts)
  expect(live.holdings).toBe(holdings)
  expect(live.brokeragePerformance.map(({ currentValue }) => currentValue)).toEqual(
    projection.brokeragePerformance.map(({ currentValue }) => currentValue),
  )
  expect(live.transactions).toBe(saved.transactions)
  expect(live.accountBalanceHistory).toBe(saved.accountBalanceHistory)
})

it('scopes restarts and ignores old updates, failures, and post-disposal callbacks', async () => {
  const update = vi.fn(),
    error = vi.fn()
  const dispose = subscribeLiveMarket(['TEST'], 10, update, error)
  const first = startCall(0)
  window.dispatchEvent(new Event('focus'))
  expect(start).toHaveBeenCalledTimes(1)
  first[3]({ ...closedMarket(), nextTransitionAt: new Date(Date.now() + 1000).toISOString() })
  update.mockClear()
  await vi.advanceTimersByTimeAsync(2000)
  const second = startCall(1)
  expect(second[2]).not.toBe(first[2])
  expect(stop).toHaveBeenCalledWith(first[2])
  const market = closedMarket()
  first[3](market)
  first[4]('obsolete')
  expect(update).not.toHaveBeenCalled()
  expect(error).not.toHaveBeenCalled()
  second[3](market)
  expect(update).toHaveBeenCalledTimes(1)
  dispose()
  expect(stop).toHaveBeenLastCalledWith(second[2])
  second[3](market)
  second[4]('late')
  window.dispatchEvent(new Event('focus'))
  await vi.advanceTimersByTimeAsync(600000)
  expect(update).toHaveBeenCalledTimes(1)
  expect(error).not.toHaveBeenCalled()
  expect(start).toHaveBeenCalledTimes(2)
})

it('waits for the market transition when closed and retries a failed start', async () => {
  const dispose = subscribeLiveMarket(['TEST'], 10, vi.fn(), vi.fn())
  startCall(0)[3]({
    ...closedMarket(),
    nextTransitionAt: new Date(Date.now() + 3600000).toISOString(),
  })
  await vi.advanceTimersByTimeAsync(3600000)
  expect(start).toHaveBeenCalledTimes(1)
  await vi.advanceTimersByTimeAsync(1000)
  expect(start).toHaveBeenCalledTimes(2)
  startCall(1)[4]('temporary failure')
  await vi.advanceTimersByTimeAsync(60000)
  expect(start).toHaveBeenCalledTimes(3)
  dispose()
})

it('stops a pending startup and discards its late rejection', async () => {
  let reject!: (error: Error) => void
  start.mockImplementationOnce(
    () =>
      new Promise((_, failed) => {
        reject = failed
      }),
  )
  const error = vi.fn()
  const dispose = subscribeLiveMarket(['TEST'], 10, vi.fn(), error)
  dispose()
  expect(stop).toHaveBeenCalledWith(startCall(0)[2])
  reject(new Error('late startup'))
  await vi.advanceTimersByTimeAsync(60000)
  expect(error).not.toHaveBeenCalled()
  expect(start).toHaveBeenCalledTimes(1)
})

function closedMarket() {
  return marketSnapshotsSchema.parse({
    snapshots: {},
    session: 'Market closed',
    feed: 'iex',
    delayMinutes: 0,
    asOf: null,
    nextTransitionAt: null,
    pollIntervalMs: null,
    historyFeed: 'iex',
    historyDelayMinutes: 0,
  })
}

it('keeps the native connection across focus and REST reconciliation cadence', async () => {
  const dispose = subscribeLiveMarket(['TEST'], 10000, vi.fn(), vi.fn())
  startCall(0)[3]({
    ...closedMarket(),
    session: 'Regular market',
    feed: 'sip',
    pollIntervalMs: 300000,
    nextTransitionAt: new Date(Date.now() + 3600000).toISOString(),
  })
  window.dispatchEvent(new Event('focus'))
  await vi.advanceTimersByTimeAsync(600000)
  expect(start).toHaveBeenCalledTimes(1)
  expect(stop).not.toHaveBeenCalled()
  dispose()
})

it('keeps annotation-aware account income and classified ledger evidence through price ticks', () => {
  const saved = { ...seedSnapshot(), revision: 4 }
  present(saved.accounts[0], 'an account').investmentIncomeYtd = 12
  const projection: MarketProjection = {
    rollup: emptyRollup(),
    ...saved,
    accounts: saved.accounts.map((account) => ({
      ...account,
      value: 100,
      investmentIncomeYtd: 999,
    })),
  }
  const live = applyLiveProjection(saved, projection)
  expect(live.accounts[0]?.value).toBe(100)
  expect(live.accounts[0]?.investmentIncomeYtd).toBe(12)
  expect(live.transactions).toBe(saved.transactions)
  expect(live.spending).toBe(saved.spending)
})

const dayChange = (change: number): ChartChanges => [
  { series: 'net-worth', windows: { day: { change, percent: change / 100 } } },
]

it('shows a stream’s changes only once its week completes the lines', () => {
  const day = dayChange
  const frame = (change: number) => ({ ...closedMarket(), chartChanges: day(change) })
  // Frames before the week read a partial line, so nothing is shown from them yet.
  let state = withMarketFrame({ series: {} }, frame(1), 'TEST', true)
  state = withMarketFrame(state, frame(2), 'TEST', false)
  expect(state.changes).toBeUndefined()
  state = withMarketWeek(state, {}, day(5), 'TEST')
  expect(state.changes).toEqual(day(5))
  state = withMarketFrame(state, frame(6), 'TEST', false)
  expect(state.changes).toEqual(day(6))
  // A restart keeps what it showed until its own week; a week without changes settles on the
  // latest frame's.
  state = withMarketFrame(state, frame(3), 'TEST', true)
  expect(state.changes).toEqual(day(6))
  state = withMarketWeek(state, {}, undefined, 'TEST')
  expect(state.changes).toEqual(day(3))
  // Another set of symbols' week is ignored.
  expect(withMarketWeek(state, {}, day(9), 'OTHER')).toBe(state)
})

/** The same points on Home's line and the accounts' line. */
const lines = (points: Array<{ time: number; value: number }>) => ({
  'net-worth': points,
  accounts: points,
})

it('only adds detail to a line through launch and a snapshot change, and resets for new symbols', () => {
  const minute = 60
  // A month of half hours, then a week of five-minute bars, ending at the last saved minute.
  const weekStart = Date.parse('2026-11-04T14:30:00Z') / 1_000
  const now = Date.parse('2026-11-11T15:00:00Z') / 1_000
  const month = Array.from({ length: 24 * 2 * 20 }, (_, index) => ({
    time: weekStart - (24 * 2 * 20 - index) * 30 * minute,
    value: 1_000 + index,
  }))
  const week = Array.from({ length: (now - weekStart) / (5 * minute) }, (_, index) => ({
    time: weekStart + index * 5 * minute,
    value: 2_000 + index,
  }))
  const line = [...month, ...week]
  const base: MarketProjection = { rollup: emptyRollup(), ...seedSnapshot(), revision: 0 }
  const frame = (revision: number, updatedAt: string, chartSeries: ReturnType<typeof lines>) => ({
    ...closedMarket(),
    chartSeries,
    projection: { ...base, revision, updatedAt },
    chartChanges: dayChange(revision),
  })
  // The previous close and now: a first frame for a holding without saved bars.
  const twoPoints = lines([
    { time: Date.parse('2026-11-10T21:00:00Z') / 1_000, value: 2_500 },
    { time: now + 90, value: 2_600 },
  ])
  const corrected = present(week.at(-10), 'a week point')
  const history = lines(
    week.map((point) => (point === corrected ? { ...point, value: point.value + 0.5 } : point)),
  )

  let previous = { first: Infinity, last: -Infinity, length: 0 }
  const expectWhole = (state: LiveMarketState) => {
    for (const id of ['net-worth', 'accounts']) {
      const times = present(state.series[id], id).map(({ time }) => time)
      // Sorted and unique, and never covering less time or fewer points than before.
      expect(times).toEqual([...new Set(times)].toSorted((left, right) => left - right))
      expect(times[0]).toBeLessThanOrEqual(previous.first)
      expect(times.at(-1)).toBeGreaterThanOrEqual(previous.last)
      expect(times.length).toBeGreaterThanOrEqual(previous.length)
    }
    const times = present(state.series.accounts, 'accounts').map(({ time }) => time)
    previous = {
      first: present(times[0], 'a first time'),
      last: present(times.at(-1), 'a last time'),
      length: times.length,
    }
  }
  const savedValue = (state: LiveMarketState) =>
    state.series.accounts?.find(({ time }) => time === corrected.time)?.value

  // Launch: the saved frame, the stream's two-point first frame, a tick, then its history.
  const saved = { ...frame(1, 'A', lines(line)), cached: true }
  let state = forSnapshot({ series: {} }, 'TEST', '1:A', saved)
  expectWhole(state)
  expect(state.series.accounts).toHaveLength(line.length)
  state = withMarketFrame(state, frame(1, 'A', twoPoints), 'TEST', true)
  expectWhole(state)
  state = withMarketFrame(
    state,
    {
      ...closedMarket(),
      chartPoint: {
        accounts: { time: now + 150, value: 2_610 },
        'net-worth': { time: now + 150, value: 2_610 },
      },
    },
    'TEST',
    false,
  )
  expectWhole(state)
  expect(state.changes).toBeUndefined()
  state = withMarketWeek(state, history, dayChange(7), 'TEST')
  expectWhole(state)
  expect(state.changes).toEqual(dayChange(7))
  // Rust's corrected value for a timestamp the line already had wins.
  expect(savedValue(state)).toBe(corrected.value + 0.5)
  // The month before the week stays; the tick after the week's last bar stays.
  expect(state.series.accounts?.[0]).toEqual(month[0])
  expect(state.series.accounts?.at(-1)).toEqual({ time: now + 120, value: 2_610 })

  // A new snapshot of the same symbols keeps the lines but none of the old values.
  const before = state.series
  state = forSnapshot(state, 'TEST', '2:B', undefined)
  expect(state.series).toBe(before)
  expect(state.market).toBeUndefined()
  expect(state.changes).toBeUndefined()
  // Settled during render: the same inputs give the same state.
  expect(forSnapshot(state, 'TEST', '2:B', undefined)).toBe(state)
  state = withMarketFrame(state, frame(2, 'B', twoPoints), 'TEST', true)
  expectWhole(state)
  expect(state.changes).toBeUndefined()
  state = withMarketWeek(
    state,
    lines(week.map((point) => ({ ...point, value: point.value * 2 }))),
    undefined,
    'TEST',
  )
  expectWhole(state)
  // The new stream's history replaces every point it spans, so no old valuation survives there.
  expect(savedValue(state)).toBe(corrected.value * 2)
  expect(state.changes).toEqual(dayChange(2))

  // Another set of symbols starts over, seeded from its own saved frame when there is one.
  expect(forSnapshot(state, 'OTHER', '2:B', undefined).series).toEqual({})
  expect(forSnapshot(state, 'OTHER', '2:B', saved).series.accounts).toHaveLength(line.length)
})

it('joins the intraday week to the frame and keeps ticks that arrived after it', () => {
  const current = {
    total: [
      { time: 600, value: 1 },
      { time: 4_200, value: 4 },
    ],
    'plaid:cash': [{ time: 600, value: 9 }],
  }
  const history = {
    total: [
      { time: 0, value: 2 },
      { time: 3_600, value: 3 },
    ],
  }
  const merged = withMarketHistory(current, history)
  // The week leads; the tick after its last bar stays, and the frame's older point goes.
  expect(merged.total).toEqual([
    { time: 0, value: 2 },
    { time: 3_600, value: 3 },
    { time: 4_200, value: 4 },
  ])
  // A series the history doesn't cover is left as it was.
  expect(merged['plaid:cash']).toEqual(current['plaid:cash'])
})
