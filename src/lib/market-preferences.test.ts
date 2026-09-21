import { describe, expect, it, vi } from 'vitest'

import {
  marketUpdateIntervals,
  parseMarketUpdateInterval,
  saveMarketUpdateInterval,
  getMarketUpdateInterval,
} from './market-preferences'

describe('market preferences', () => {
  it('defaults to ten seconds and accepts only supported update intervals', () => {
    expect(marketUpdateIntervals.map(({ seconds }) => seconds)).toEqual([5, 10, 30, 60])
    expect(parseMarketUpdateInterval(null)).toBe(10)
    expect(parseMarketUpdateInterval('10')).toBe(10)
    expect(parseMarketUpdateInterval('60')).toBe(60)
    expect(parseMarketUpdateInterval('1')).toBe(10)
  })
})

it('announces same-window cadence edits immediately', () => {
  const target = new EventTarget()
  let stored: string | null = null
  vi.stubGlobal(
    'window',
    Object.assign(target, {
      localStorage: {
        getItem: () => stored,
        setItem: (_key: string, value: string) => {
          stored = value
        },
      },
    }),
  )
  try {
    const onChange = vi.fn(() => expect(getMarketUpdateInterval()).toBe(30))
    target.addEventListener('brief.marketUpdateIntervalSeconds', onChange)
    saveMarketUpdateInterval(30)
    expect(onChange).toHaveBeenCalledTimes(1)
  } finally {
    vi.unstubAllGlobals()
  }
})
