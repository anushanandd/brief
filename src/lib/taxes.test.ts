import { expect, it } from 'vitest'

import { bankDetails } from '../components/transaction-dialog'
import { classification } from '../data/fixtures/classification'
import { estimateRange, parseAmount, taxYear } from './taxes'

const estimate = {
  low: 100,
  high: 100,
  ordinary: 60,
  capitalGains: 40,
  niit: 0,
  state: 0,
  marginalRate: 22,
  capitalGainsRate: 15,
  lossDeduction: 0,
  lossCarryover: 0,
}

it('shows one estimate when every dividend is classified and a range otherwise', () => {
  expect(estimateRange(estimate)).toEqual({ single: 100 })
  expect(estimateRange({ ...estimate, high: 140 })).toEqual({ low: 100, high: 140 })
})

const year = (value: number) => ({
  year: value,
  interest: 0,
  dividends: { total: 0, qualified: 0, nonQualified: 0, unclassified: 0 },
  realized: {
    shortTerm: 0,
    longTerm: 0,
    washDisallowed: 0,
    sales: 0,
    unestimatedSales: 0,
    unestimatedProceeds: 0,
  },
  washSales: [],
  tablesAvailable: true,
  partial: false,
})

it('picks the requested tax year and falls back to the latest', () => {
  const taxes = {
    years: [year(2026), year(2025)],
    harvest: [],
    washWindows: [],
    hasProfile: false,
    tablesVersion: '',
    sources: [],
  }
  expect(taxYear(taxes, 2025)?.year).toBe(2025)
  expect(taxYear(taxes, 1999)?.year).toBe(2026)
  expect(taxYear(undefined)).toBeUndefined()
})

it('reads typed amounts and rates and rejects anything else', () => {
  expect(parseAmount('$185,000')).toBe(185_000)
  expect(parseAmount('4.5%')).toBe(4.5)
  expect(parseAmount('')).toBeUndefined()
  expect(parseAmount('-5')).toBeUndefined()
  expect(parseAmount('lots')).toBeUndefined()
})

it('lists bank details with Plaid confidence in words', () => {
  const rows = bankDetails({
    id: 't',
    merchant: 'Cafe',
    category: 'Dining',
    date: '2026-09-01',
    amount: -5,
    account: 'Card',
    pending: false,
    classification: classification('expense'),
    originalDescription: 'SQ *CAFE 0412',
    checkNumber: '1042',
    categoryConfidence: 'VERY_HIGH',
    counterparties: [{ name: 'Square', type: 'payment_app', confidence: 'HIGH' }],
    location: { storeNumber: '0412' },
  })
  expect(rows.map(({ label, value }) => `${label}: ${value}`)).toEqual([
    'Raw text: SQ *CAFE 0412',
    'Payment app: Square · high confidence',
    'Check: No. 1042',
    'Store: No. 0412',
    'Category: Plaid · very high confidence',
  ])
})
