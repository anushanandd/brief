import { describe, expect, it } from 'vitest'

import { appendLiveChartPoint, buildLiveChartData, chartPointAtOrAfter } from './live-chart'

describe('live chart points', () => {
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

  it('anchors an event to the next plotted value', () => {
    const points = [
      { time: 10, value: 100 },
      { time: 20, value: 110 },
    ]
    expect(chartPointAtOrAfter(points, 15)).toEqual(points[1])
    expect(chartPointAtOrAfter(points, 25)).toEqual(points[1])
  })

  it('merges provider history with captured live account values', () => {
    const augustSecond = Date.parse('2026-08-02T18:00:00Z') / 1_000
    const augustThird = Date.parse('2026-08-03T18:00:00Z') / 1_000
    const data = buildLiveChartData(
      [
        { date: '2026-08-01', value: 90 },
        { date: '2026-08-02', value: 95 },
      ],
      [{ time: augustSecond, value: 100 }],
      105,
      augustThird,
    )

    expect(data.map((point) => point.value)).toEqual([90, 100, 105])
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
