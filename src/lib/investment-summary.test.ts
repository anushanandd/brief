import { expect, it } from 'vitest'

import { insights } from '../data/fixtures/insights'
import nativeSummary from '../data/fixtures/native-summary.json'
import { seedSnapshot } from '../data/fixtures/records'
import { summaryStart } from './investment-summary'
import { fromRust } from './rust-json'
import type { InvestmentSummary } from './schema'

// Rust's own serialization of a summary, checked from the Rust side.
const summary = fromRust<InvestmentSummary>(nativeSummary)

it('carries Rust’s brief whole, line by line, with its headlines as published', () => {
  const brief = summary.brief!
  // Every word is Rust’s: the renderer joins none of its own.
  const text = brief.lines[0].map((span) => span.text).join('')
  expect(text).toMatch(/^TEST rose 2\.4% \(\+\$49\) today, more than the S&P 500’s 0\.\d%\./)
  expect(brief.headlines[0]).toEqual({
    tickers: ['TEST'],
    title: 'Test Corp jumps after naming a new finance chief',
    publisher: 'CNBC',
    url: 'https://example.com/cfo',
    publishedAt: '2026-09-30T13:00:00Z',
  })
})

it('starts a summary where the portfolio’s reconciled period does, or the day before the range', () => {
  const data = seedSnapshot()
  const month = {
    window: 'month' as const,
    start: '2026-09-06',
    end: '2026-10-07',
    startValue: 1000,
    endValue: 1010,
    deposits: 0,
    withdrawals: 0,
    netFlows: 0,
    investmentGain: 10,
    timeWeightedPct: 1,
    moneyWeightedPct: 1,
    annualized: null,
    benchmarkPct: null,
    activePct: null,
    waterfall: {
      start: 1000,
      deposits: 0,
      withdrawals: 0,
      realized: 0,
      unrealized: 10,
      priceGain: 10,
      dividends: 0,
      interest: 0,
      fees: 0,
      taxes: 0,
      end: 1010,
    },
    risk: { drawdownPct: null, volatilityPct: null, beta: null },
  }
  const withPeriods = {
    ...data,
    insights: insights({ today: '2026-10-07', periods: [month] }),
  }
  expect(summaryStart(withPeriods, 'month')).toBe('2026-09-06')
  // Without a period, the week starts the day before its first day.
  expect(summaryStart(withPeriods, 'week')).toBe('2026-09-30')
  expect(summaryStart(withPeriods, 'all')).toBeUndefined()
})
