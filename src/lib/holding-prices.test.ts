import { expect, it } from 'vitest'

import {
  cachedHoldingPreview,
  holdingChartPoints,
  holdingChartCandles,
  holdingChartTimeline,
  holdingSessions,
  visibleHoldingChartPoints,
  holdingChartTime,
  holdingRangeChange,
  holdingChartEnd,
  reduceHoldingChart,
  parseHoldingChartRange,
  type HoldingChartState,
} from './holding-prices'
import { holdingChartEventSchema, type HoldingPriceHistory, type PriceBar } from './schema'

const t = (value: string) => Date.parse(value) / 1000
const identity = { symbol: 'TEST', requestId: 'test-request' }

it('keeps same-day times compact without hiding older observation dates', () => {
  const today = new Date(2026, 8, 18, 12, 9, 22).getTime() / 1000
  const yesterday = new Date(2026, 8, 17, 16, 44).getTime() / 1000
  expect(holdingChartTime(today, today, true)).toBe(
    new Date(today * 1000).toLocaleString(undefined, {
      hour: 'numeric',
      minute: '2-digit',
      second: '2-digit',
    }),
  )
  expect(holdingChartTime(yesterday, today)).toBe(
    new Date(yesterday * 1000).toLocaleString(undefined, {
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
    }),
  )
})
const now = Date.parse('2026-09-17T02:00:00Z') / 1000
const bar = (time: number, close = 100, feed: 'sip' | 'boats' = 'sip'): PriceBar => ({
  time,
  endTime: time + 60,
  open: close,
  high: close,
  low: close,
  close,
  volume: 10,
  trades: 2,
  feed,
})
const history = (
  bars: PriceBar[],
  options: Partial<HoldingPriceHistory> = {},
): HoldingPriceHistory => ({
  symbol: 'TEST',
  range: 86400,
  resolution: 60,
  start: now - 86460,
  end: now - 902,
  fetchedAt: now * 1000,
  bars,
  feeds: ['sip', 'boats'],
  delayMinutes: 15,
  adjustment: 'split',
  cached: false,
  ...options,
})
const state = (
  bars: PriceBar[],
  options: Partial<HoldingPriceHistory> = {},
): HoldingChartState => ({
  symbol: 'TEST',
  range: 86400,
  bars: [],
  history: history(bars, options),
})

it('defaults to day and accepts only supported saved Holdings ranges', () => {
  for (const value of [undefined, null, '', 'invalid', false, {}, -1, 123])
    expect(parseHoldingChartRange(value)).toBe(86400)
  for (const value of [86400, 604800, 2592000, 31536000, 0])
    expect(parseHoldingChartRange(String(value))).toBe(value)
})

it('cold overnight history retains daytime and overnight executions and their gaps', () => {
  const points = holdingChartPoints(
    state([
      bar(now - 36000),
      bar(now - 35940, 101),
      bar(now - 6000, 102, 'boats'),
      bar(now - 5880, 103, 'boats'),
    ]),
  )
  expect(points.map((point) => point.value)).toEqual([100, 101, 102, 103])
  expect(points.map((point) => point.breakBefore)).toEqual([false, false, true, true])
  expect(points[0].time).toBe(now - 35940)
})

it('never splices an indicative midpoint or an individual trade into bar history', () => {
  const initial = state([bar(now - 1200), bar(now - 1140, 101)])
  for (const indicative of [true, false]) {
    const next = reduceHoldingChart(initial, {
      ...identity,
      kind: 'quote',
      price: 500,
      time: now,
      feed: 'overnight',
      indicative,
    })
    expect(next.quote?.price).toBe(500)
    expect(holdingChartPoints(next)).toEqual(holdingChartPoints(initial))
    const older = reduceHoldingChart(next, {
      ...identity,
      kind: 'quote',
      price: 2,
      time: now - 1,
      feed: 'overnight',
      indicative,
    })
    expect(older.quote?.price).toBe(500)
  }
})

it('displays REST history before subscription and reconciles the connection gap afterward', () => {
  let current = reduceHoldingChart(
    { symbol: 'TEST', range: 86400, bars: [] },
    {
      ...identity,
      kind: 'history',
      history: history([bar(now - 1200, 100)]),
      startedAt: now * 1000,
    },
  )
  expect(holdingChartPoints(current).map((point) => point.value)).toEqual([100])
  expect(current.connection).toBeUndefined()
  current = reduceHoldingChart(current, {
    ...identity,
    kind: 'status',
    status: 'subscribed',
    session: 'Regular',
    feed: 'sip',
    delayMinutes: 0,
  })
  current = reduceHoldingChart(current, {
    ...identity,
    kind: 'bar',
    bar: bar(now - 60, 103),
    receivedAt: now * 1000 + 3,
  })
  current = reduceHoldingChart(current, {
    ...identity,
    kind: 'history',
    history: history([bar(now - 1200, 100), bar(now - 120, 102)], { end: now }),
    startedAt: now * 1000 + 2,
  })
  expect(holdingChartPoints(current).map((point) => point.value)).toEqual([100, 102, 103])
})

it('applies older corrected bars and preserves events received during REST and beyond its cutoff', () => {
  let current = state([bar(now - 1200), bar(now - 1140, 101)])
  current = reduceHoldingChart(current, {
    ...identity,
    kind: 'bar',
    bar: bar(now - 1200, 98),
    receivedAt: now * 1000 + 1,
  })
  current = reduceHoldingChart(current, {
    ...identity,
    kind: 'bar',
    bar: bar(now - 900, 102),
    receivedAt: now * 1000 - 1,
  })
  current = reduceHoldingChart(current, {
    ...identity,
    kind: 'history',
    history: history([bar(now - 1200), bar(now - 1140, 101)]),
    startedAt: now * 1000,
  })
  expect(holdingChartPoints(current).map((point) => point.value)).toEqual([98, 101, 102])
  const repaired = reduceHoldingChart(current, {
    ...identity,
    kind: 'history',
    history: history([bar(now - 1140, 101), bar(now - 900, 99)], { end: now }),
    startedAt: now * 1000 + 2,
  })
  expect(holdingChartPoints(repaired).map((point) => point.value)).toEqual([101, 99])
})

it('corrections clear the provisional latest observation and trigger canonical repair without mixing feeds', () => {
  const initial = state([bar(now - 1200)])
  initial.bars = [
    { ...bar(now - 900), receivedAt: now * 1000 },
    { ...bar(now - 800, 90, 'boats'), receivedAt: now * 1000 },
  ]
  const next = reduceHoldingChart(initial, { ...identity, kind: 'invalidate', feed: 'delayed_sip' })
  expect(next.bars.map((b) => b.feed)).toEqual(['boats'])
  expect(next.quote).toBeUndefined()
})

it('coarser ranges never mix minute samples with daily or multi-minute closes', () => {
  const daily = { ...bar(now - 86400), endTime: now - 28800 }
  const initial = state([daily], { resolution: 86400, range: 31536000, feeds: ['sip'] })
  initial.bars = [{ ...bar(now - 120, 200), receivedAt: now * 1000 }]
  expect(holdingChartPoints(initial)).toEqual([
    { time: daily.endTime, value: 100, breakBefore: false },
  ])
})

it('suppresses unsupported changes, never interpolating a missing left boundary', () => {
  const initial = state([bar(now - 86460, 100), bar(now - 1200, 110)])
  const points = holdingChartPoints(initial)
  expect(holdingRangeChange(initial, points, now)).toEqual({ change: 10, percent: 10 })
  expect(holdingRangeChange({ ...initial, historyError: 'Failed' }, points, now)).toBeUndefined()
  expect(
    holdingRangeChange(state(initial.history!.bars, { cached: true }), points, now),
  ).toBeUndefined()
  expect(holdingRangeChange(state(initial.history!.bars), points.slice(1), now)).toBeUndefined()
  const shorter = state([bar(now - 7200), bar(now - 1200)])
  expect(holdingRangeChange(shorter, holdingChartPoints(shorter), now)).toBeUndefined()
})

it('retains validated saved data on errors and rejects malformed IPC bars', () => {
  const initial = state([bar(now - 1200)])
  const failed = reduceHoldingChart(initial, {
    ...identity,
    kind: 'history-error',
    message: 'Offline',
  })
  expect(failed.history).toBe(initial.history)
  expect(
    holdingChartEventSchema.safeParse({
      ...identity,
      kind: 'bar',
      bar: { ...bar(now), close: -1 },
      receivedAt: now,
    }).success,
  ).toBe(false)
})

it('advances the visible window without changing canonical points', () => {
  const points = holdingChartPoints(state([bar(now - 86400), bar(now - 1200, 110)]))
  expect(visibleHoldingChartPoints(points, 86400, now)).toHaveLength(2)
  expect(visibleHoldingChartPoints(points, 86400, now + 120)).toEqual([points[1]])
  expect(visibleHoldingChartPoints(points, 0, now + 120)).toEqual(points)
  expect(points).toHaveLength(2)
})

it('uses an observed close before a weekend boundary without inventing a boundary price', () => {
  const sunday = Date.parse('2026-09-20T16:00:00Z') / 1000
  const priorFriday = Date.parse('2026-09-11T20:00:00Z') / 1000
  const friday = Date.parse('2026-09-18T20:00:00Z') / 1000
  const weekly = state([bar(priorFriday - 60, 100), bar(friday - 60, 112)], { range: 7 * 86400 })
  expect(holdingRangeChange(weekly, holdingChartPoints(weekly), sunday)).toEqual({
    change: 12,
    percent: 12,
  })
  expect(
    holdingRangeChange(weekly, holdingChartPoints(weekly), sunday + 14 * 86400),
  ).toBeUndefined()
})

it('anchors a closed Day view to the last observed day, never shifting an open or stale view', () => {
  const sunday = Date.parse('2026-09-20T16:00:00Z') / 1000
  const friday = Date.parse('2026-09-18T20:00:00Z') / 1000
  const closed: HoldingChartState = {
    ...state([bar(friday - 86460, 100), bar(friday - 3600, 105), bar(friday - 60, 110)]),
    connection: {
      ...identity,
      kind: 'status',
      status: 'closed',
      session: 'Market closed',
      feed: 'sip',
      delayMinutes: 0,
    },
  }
  const points = holdingChartPoints(closed)
  expect(holdingChartEnd(closed, points, sunday)).toBe(friday)
  expect(holdingRangeChange(closed, points, sunday)).toEqual({ change: 10, percent: 10 })
  expect(
    visibleHoldingChartPoints(points, 86400, holdingChartEnd(closed, points, sunday)),
  ).toHaveLength(3)
  expect(holdingChartEnd({ ...closed, connection: undefined }, points, sunday)).toBe(sunday)
  expect(holdingChartEnd(closed, points, sunday + 8 * 86400)).toBe(sunday + 8 * 86400)
  expect(holdingRangeChange({ ...closed, historyError: 'Failed' }, points, sunday)).toBeUndefined()
})

it('restores only matching same-day canonical history as saved, without live observations', () => {
  const saved = history([bar(now - 60), bar(now)], { fetchedAt: now * 1000 })
  const preview = cachedHoldingPreview(saved, 'TEST', 86400, now * 1000 + 1000)
  expect(preview.history?.cached).toBe(true)
  expect(preview.bars).toEqual([])
  expect(preview.quote).toBeUndefined()
  expect(preview.connection).toBeUndefined()
  expect(holdingChartPoints(preview)).toHaveLength(2)
  expect(holdingRangeChange(preview, holdingChartPoints(preview), now)).toBeUndefined()
  expect(cachedHoldingPreview(saved, 'OTHER', 86400, now * 1000).history).toBeUndefined()
  expect(cachedHoldingPreview(saved, 'TEST', 0, now * 1000).history).toBeUndefined()
  expect(cachedHoldingPreview(saved, 'TEST', 86400, (now + 86400) * 1000).history).toBeUndefined()
  expect(cachedHoldingPreview(saved, 'TEST', 86400, now * 1000 - 1).history).toBeUndefined()
})

it('retains an explicitly saved comparison at its verified endpoint and clears it on correction', () => {
  const verifiedHistory = history([bar(now - 86460, 100), bar(now - 1200, 110)], {
    fetchedAt: now * 1000,
  })
  const verified = reduceHoldingChart(
    { symbol: 'TEST', range: 86400, bars: [] },
    {
      kind: 'history',
      symbol: 'TEST',
      requestId: 'first',
      history: verifiedHistory,
      startedAt: now * 1000,
    },
  )
  const preview = cachedHoldingPreview(
    { ...verifiedHistory, savedComparison: verified.savedComparison },
    'TEST',
    86400,
    now * 1000 + 1000,
  )
  expect(holdingRangeChange(preview, holdingChartPoints(preview), now + 3600)).toEqual({
    change: 10,
    percent: 10,
  })
  expect(preview.savedComparison?.asOf).toBe(now * 1000)
  const failed = reduceHoldingChart(verified, {
    kind: 'history-error',
    symbol: 'TEST',
    requestId: 'first',
    message: 'offline',
  })
  expect(holdingRangeChange(failed, holdingChartPoints(failed), now + 3600)).toEqual({
    change: 10,
    percent: 10,
  })
  const corrected = reduceHoldingChart(preview, {
    kind: 'invalidate',
    symbol: 'TEST',
    requestId: 'first',
    feed: 'sip',
  })
  expect(holdingRangeChange(corrected, holdingChartPoints(corrected), now)).toBeUndefined()
})

it('compresses the weekend while preserving real timestamps, feed boundaries and DST', () => {
  const friday = t('2026-03-06T21:00:00Z')
  const sunday = t('2026-03-09T00:01:00Z')
  const h = history([bar(friday - 60), bar(sunday - 60, 101, 'boats')], {
    range: 604800,
    start: friday - 86400,
    end: sunday,
    calendar: [
      { date: '2026-03-06', open: 34200, close: 57600 },
      { date: '2026-03-09', open: 34200, close: 57600 },
    ],
  })
  const points = holdingChartPoints({ history: h, bars: [] })
  const axis = holdingChartTimeline(h, points, sunday, 604800)
  expect(axis.gaps).toHaveLength(1)
  expect(axis.toDisplay(sunday) - axis.toDisplay(friday)).toBe(4 * 3600 + 120)
  expect(axis.toActual(axis.toDisplay(sunday))).toBe(sunday)
  expect(axis.points[1].breakBefore).toBe(true)
  expect(points[1].time).toBe(sunday)
  expect(
    holdingSessions(h).find((session) => session.start === t('2026-03-09T13:30:00Z'))?.label,
  ).toBe('Open')
})

it('connects only fully fetched same-feed history and leaves unknown/live gaps visible', () => {
  const bars = [bar(now - 3600), bar(now - 1800)]
  const h = history(bars, { calendar: [{ date: '2026-09-16', open: 34200, close: 57600 }] })
  expect(holdingChartPoints({ history: h, bars: [] })[1].breakBefore).toBe(false)
  expect(
    holdingChartPoints({ history: h, bars: [], historyError: 'Correction pending' })[1].breakBefore,
  ).toBe(true)
  expect(
    holdingChartPoints({ history: { ...h, calendar: undefined }, bars: [] })[1].breakBefore,
  ).toBe(true)
  expect(
    holdingChartPoints({ history: h, bars: [{ ...bar(now - 60), receivedAt: now * 1000 }] }).at(-1)
      ?.breakBefore,
  ).toBe(true)
})

it('honors early closes, holidays and preserves unexpected observations inside closures', () => {
  const h = history([], {
    resolution: 86400,
    calendar: [
      { date: '2026-12-24', open: 34200, close: 46800 },
      { date: '2026-12-28', open: 34200, close: 57600 },
    ],
  })
  const first = t('2026-12-24T18:00:00Z')
  const end = t('2026-12-28T21:00:00Z')
  const points = [
    { time: first, value: 100 },
    { time: end, value: 101 },
  ]
  const axis = holdingChartTimeline(h, points, end, 31536000)
  expect(holdingSessions(h)[2].end).toBe(first)
  expect(axis.toDisplay(end) - axis.toDisplay(first)).toBe(7.5 * 3600)
  expect(axis.toActual(axis.toDisplay(first))).toBe(first)
  expect(axis.points[1].breakBefore).toBeFalsy()
  const unexpected = [...points, { time: first + 86400, value: 102 }].toSorted(
    (a, b) => a.time - b.time,
  )
  expect(holdingChartTimeline(h, unexpected, end, 31536000).gaps).toHaveLength(0)
  expect(
    holdingChartTimeline({ ...h, calendar: undefined }, points, end, 31536000).gaps,
  ).toHaveLength(0)
})

it('builds candle OHLC from canonical bars and corrections without mixing feeds or quotes', () => {
  const start = t('2026-09-16T14:00:00Z')
  const original = { ...bar(start, 101), open: 100, high: 103, low: 99 }
  const corrected = { ...bar(start + 60, 104), open: 101, high: 106, low: 100 }
  const h = history([original, bar(start + 60, 102), bar(start + 120, 105, 'boats')], {
    start,
    end: start + 900,
    fetchedAt: (start + 900) * 1000,
  })
  const current: HoldingChartState = {
    history: h,
    bars: [{ ...corrected, receivedAt: (start + 180) * 1000 }],
  }
  const points = holdingChartPoints(current)
  const timeline = holdingChartTimeline(h, points, start + 900, 86400)
  const result = holdingChartCandles(current, timeline, start + 900, 86400)
  expect(result.candles).toHaveLength(2)
  expect(result.candles[0]).toMatchObject({ open: 100, high: 106, low: 99, close: 104 })
  expect(
    timeline.toActual(result.candles[0].time + (result.candles[0].width ?? result.width) / 2),
  ).toBe(start + 120)
  expect(h.bars[1].close).toBe(102)
  expect(
    holdingChartCandles(
      { ...current, history: { ...h, cached: true } },
      timeline,
      start + 900,
      86400,
    ).candles[0].close,
  ).toBe(102)
})

it('does not merge candles across intraday sessions or include out-of-range bars', () => {
  const start = t('2026-09-16T13:29:00Z')
  const h = history([bar(start), bar(start + 60, 102), bar(start + 120, 103)], {
    calendar: [{ date: '2026-09-16', open: 34200, close: 57600 }],
    start,
    end: start + 180,
  })
  const current = { history: h, bars: [] }
  const timeline = holdingChartTimeline(h, holdingChartPoints(current), start + 120, 2592000)
  const result = holdingChartCandles(current, timeline, start + 120, 2592000)
  expect(result.candles).toHaveLength(2)
  expect(result.candles.map((c) => c.close)).toEqual([100, 102])
})

it('keeps ordinary candle bodies wide when the last candle is only one minute old', () => {
  const start = t('2026-09-16T14:00:00Z')
  const bars = Array.from({ length: 61 }, (_, index) => bar(start + index * 60, 100 + index))
  const current = state(bars, { start, end: start + 3660 })
  const timeline = holdingChartTimeline(
    current.history,
    holdingChartPoints(current),
    start + 3660,
    86400,
  )
  const result = holdingChartCandles(current, timeline, start + 3660, 86400)
  expect(result.candles[0].width).toBeGreaterThan(result.candles.at(-1)!.width!)
  expect(result.candles[0].width).toBe(result.resolution)
  result.candles.forEach((candle, index) => {
    const center = timeline.toActual(candle.time + candle.width! / 2)
    expect(bars.some((point) => point.endTime === center)).toBe(true)
    if (index)
      expect(result.candles[index - 1].time + result.candles[index - 1].width!).toBeLessThanOrEqual(
        candle.time,
      )
  })
})
