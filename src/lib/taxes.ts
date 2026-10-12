import type { Taxes, TaxYear } from './schema'

/** How an account's income and gains are taxed, as the interface names it. */
export const taxTreatmentLabels = {
  taxable: 'Taxable',
  taxDeferred: 'Tax-deferred',
  taxFree: 'Tax-free',
} as const

/** The tax year a report shows: the requested one when Rust reported it, else the latest. */
export function taxYear(taxes: Taxes | undefined, year?: number): TaxYear | undefined {
  return taxes?.years.find((entry) => entry.year === year) ?? taxes?.years[0]
}

/** A Rust estimate as one figure, or a range while dividends await classification. */
export function estimateRange(estimate: NonNullable<TaxYear['estimate']>) {
  return estimate.low === estimate.high
    ? { single: estimate.high }
    : { low: estimate.low, high: estimate.high }
}

/** Reads "185,000", "$185000", or "4.5%" as a number; blank or invalid is `undefined`. */
export function parseAmount(text: string) {
  const cleaned = text.replace(/[$,%\s]/g, '')
  if (!cleaned) return undefined
  const value = Number(cleaned)
  return Number.isFinite(value) && value >= 0 ? value : undefined
}
