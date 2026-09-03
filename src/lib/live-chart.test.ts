import { describe, expect, it } from 'vitest'

import { appendLiveChartPoint, buildLiveChartData } from './live-chart'

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
})
