import { describe, expect, it } from 'vitest'

import {
  asciiMinus,
  changeTone,
  countOf,
  displayCase,
  formatActivityName,
  formatCompactCurrency,
  formatCurrency,
  formatPercent,
  formatSecurityName,
  formatShare,
  formatSignedCompactCurrency,
  formatSignedCurrency,
  MINUS,
  pluralize,
} from './format'

describe('financial formatting', () => {
  it('formats currency with cents and a typographic minus', () => {
    expect(formatCurrency(2418.33)).toBe('$2,418.33')
    expect(formatCurrency(-54.2)).toBe(`${MINUS}$54.20`)
    expect(asciiMinus(formatCurrency(-54.2))).toBe('-$54.20')
  })

  it('formats compact dashboard values', () => {
    expect(formatCompactCurrency(286421)).toBe('$286.4K')
  })

  it('preserves the sign of percentage movement', () => {
    expect(formatPercent(0.65)).toBe('+0.65%')
    expect(formatPercent(-0.31)).toBe(`${MINUS}0.31%`)
    expect(formatPercent(0)).toBe('0.00%')
    expect(formatPercent(-0)).toBe('0.00%')
  })

  it('signs currency changes with the same minus as every amount', () => {
    expect(formatSignedCurrency(12.5)).toBe('+$12.50')
    expect(formatSignedCurrency(-12.5)).toBe(`${MINUS}$12.50`)
    expect(formatSignedCurrency(0)).toBe('$0.00')
    expect(formatSignedCurrency(null)).toBe('—')
  })

  it('keeps strip changes short without dropping cents from small ones', () => {
    expect(formatSignedCompactCurrency(12.5)).toBe('+$12.50')
    expect(formatSignedCompactCurrency(-12345.67)).toBe(`${MINUS}$12.3K`)
    expect(formatSignedCompactCurrency(1500)).toBe('+$1.5K')
  })

  it('tones changes by whether a rise is favorable', () => {
    expect(changeTone(5)).toBe('positive')
    expect(changeTone(5, 'decrease')).toBe('negative')
    expect(changeTone(0, 'decrease')).toBe('muted')
    expect(changeTone(null)).toBe('muted')
  })

  it('gives shares of a whole one unsigned decimal', () => {
    expect(formatShare(43.68)).toBe('43.7%')
    expect(formatShare(-2.5)).toBe('2.5%')
    expect(formatShare(null)).toBe('—')
  })

  it('cases shouted bank descriptors as names and keeps short codes', () => {
    expect(displayCase('ACH RENT PAYMENT')).toBe('ACH Rent Payment')
    expect(displayCase('AMERICAN EXPRESS ACH PMT')).toBe('American Express ACH PMT')
    expect(displayCase('AUTOPAY PAYMENT - THANK YOU')).toBe('Autopay Payment - Thank You')
    expect(displayCase("Trader Joe's")).toBe("Trader Joe's")
    expect(displayCase('PG&E')).toBe('PG&E')
    expect(formatActivityName('WHOLEFDS MKT 10456 AUSTIN TX STORE 7')).toBe(
      'Wholefds MKT 10456 Austin TX…',
    )
  })

  it('counts with the right noun', () => {
    expect(countOf(1, 'account')).toBe('1 account')
    expect(countOf(2, 'account')).toBe('2 accounts')
    expect(countOf(1, 'match', 'matches')).toBe('1 match')
    expect(pluralize(3, 'entry', 'entries')).toBe('entries')
  })

  it('removes share-class details from security names', () => {
    expect(formatSecurityName('Berkshire Hathaway Inc Class B')).toBe('Berkshire Hathaway Inc')
    expect(formatSecurityName('Vanguard Total Stock Market ETF')).toBe(
      'Vanguard Total Stock Market ETF',
    )
  })
})
