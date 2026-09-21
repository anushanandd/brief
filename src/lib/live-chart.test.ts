import { describe, expect, it } from 'vitest'

import {
  appendLiveChartPoint,
  buildLiveChartData,
  chartPointAtOrAfter,
  chartPointsFromStartDate,
  chartRangeChange,
  marketClosePoint,
  upsertLiveChartPoint,
} from './live-chart'

describe('live chart points', () => {
  it('keeps a single provider close point visible outside regular hours', () => {
    const close = { time: 100, value: 1_000 }

    expect(marketClosePoint([close], 'Overnight', close.time)).toBe(close)
    expect(marketClosePoint([close], 'After hours', close.time)).toBe(close)
    expect(marketClosePoint([close], 'Pre-market', close.time)).toBe(close)
    expect(marketClosePoint([close], 'Market closed', close.time)).toBe(close)
    expect(marketClosePoint([close], 'Regular market', close.time)).toBeUndefined()
    expect(marketClosePoint([close], undefined, close.time)).toBeUndefined()
  })

  it('keeps the latest account value for each minute', () => {
    const first = appendLiveChartPoint([], 100, 10)
    const sameMinute = appendLiveChartPoint(first, 101, 59)
    const nextMinute = appendLiveChartPoint(sameMinute, 102, 60)

    expect(sameMinute).toEqual([{ time: 0, value: 101 }])
    expect(nextMinute).toEqual([
      { time: 0, value: 101 },
      { time: 60, value: 102 },
    ])
    expect(appendLiveChartPoint(nextMinute, 99, 10)).toBe(nextMinute)
  })

  it('replaces a corrected minute bar without disturbing newer points', () => {
    const points = [
      { time: 60, value: 100 },
      { time: 120, value: 102 },
    ]

    expect(upsertLiveChartPoint(points, 101, 75)).toEqual([
      { time: 60, value: 101 },
      { time: 120, value: 102 },
    ])
  })

  it('anchors an event to the next plotted value', () => {
    const points = [
      { time: 10, value: 100 },
      { time: 20, value: 110 },
    ]
    expect(chartPointAtOrAfter(points, 15)).toEqual(points[1])
    expect(chartPointAtOrAfter(points, 25)).toEqual(points[1])
  })

  it('connects the closing value to the live value at its actual market time', () => {
    const augustSecond = Date.parse('2026-08-02T18:00:00Z') / 1_000
    const augustThird = Date.parse('2026-08-03T18:00:00Z') / 1_000
    const data = buildLiveChartData(
      [
        { date: '2026-08-01', value: 90 },
        { date: '2026-08-02', value: 95 },
      ],
      [
        { time: augustSecond, value: 100 },
        { time: augustThird, value: 105 },
      ],
      105,
      augustThird,
    )

    expect(data).toEqual([
      { time: Date.parse('2026-08-01T12:00:00Z') / 1_000, value: 90 },
      { time: augustSecond, value: 100 },
      { time: augustThird, value: 105 },
    ])
  })

  it('starts chart data at the saved or inferred account date', () => {
    const points = [
      { time: Date.parse('2024-12-31T12:00:00Z') / 1_000, value: 90 },
      { time: Date.parse('2025-01-01T12:00:00Z') / 1_000, value: 100 },
      { time: Date.parse('2025-01-02T12:00:00Z') / 1_000, value: 110 },
    ]

    expect(chartPointsFromStartDate(points, '2025-01-01')).toEqual(points.slice(1))
    expect(chartPointsFromStartDate(points)).toBe(points)
  })

  it('calculates change from the interpolated selected-range boundary', () => {
    const points = [
      { time: 0, value: 100 },
      { time: 10, value: 120 },
      { time: 20, value: 130 },
    ]

    const change = chartRangeChange(points, 15, 20)
    expect(change.change).toBe(20)
    expect(change.percent).toBeCloseTo(20 / 1.1)
  })

  it('calculates all-time change from the first plotted value', () => {
    expect(
      chartRangeChange(
        [
          { time: 10, value: 80 },
          { time: 20, value: 100 },
        ],
        0,
        20,
      ),
    ).toEqual({ change: 20, percent: 25 })
  })

  it('preserves zero and negative historical balances', () => {
    expect(
      buildLiveChartData(
        [
          { date: '2026-08-01', value: 0 },
          { date: '2026-08-02', value: -5 },
          { date: '2026-08-03', value: 10 },
        ],
        [],
        12,
        Date.parse('2026-08-04T00:00:00Z') / 1000,
      ).map((point) => point.value),
    ).toEqual([0, -5, 10, 12])
  })
})

it('does not fabricate a second observation from one current balance', () => {
  const now = Date.parse('2026-09-03T12:00:00Z') / 1_000
  expect(buildLiveChartData([{ date: '2026-09-03', value: 100 }], [], 100, now)).toEqual([
    { time: now, value: 100 },
  ])
})
