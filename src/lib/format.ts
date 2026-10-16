const currencyFormatter = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  maximumFractionDigits: 2,
})
const compactCurrencyFormatter = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  notation: 'compact',
  maximumFractionDigits: 1,
})
const wholeCurrencyFormatter = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  maximumFractionDigits: 0,
})
const shortDayFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  timeZone: 'UTC',
})
const dayFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
  timeZone: 'UTC',
})
/** A calendar day, `YYYY-MM-DD`, as "Sep 5". */
export const formatShortDay = (date: string) =>
  shortDayFormatter.format(new Date(`${date}T00:00:00Z`))
/** A calendar day, `YYYY-MM-DD`, as "Sep 5, 2026". */
export const formatDay = (date: string) => dayFormatter.format(new Date(`${date}T00:00:00Z`))
const updatedAtFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
  hour: 'numeric',
  minute: '2-digit',
})
const currentYearUpdatedAtFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  hour: 'numeric',
  minute: '2-digit',
})
/**
 * Negative figures carry a true minus (U+2212): in tabular figures it shares the plus sign's
 * width, so signed columns line up. Exports and copies translate it back to a hyphen.
 */
export const MINUS = '−'
const typographicMinus = (text: string) => text.replace(/^-/, MINUS)
/** The ASCII form, for clipboards and files that parse numbers. */
export const asciiMinus = (text: string) => text.replaceAll(MINUS, '-')

export const formatCurrency = (value: number | null | undefined) =>
  value == null ? '—' : typographicMinus(currencyFormatter.format(value))

/**
 * Sentences round: whole dollars and one-decimal percents. Figures in rows, metrics, and
 * selectors keep cents and two decimals. Both use the same minus.
 */
export const formatProseCurrency = (value: number) =>
  typographicMinus(wholeCurrencyFormatter.format(value))
export const formatProsePercent = (value: number) => `${Math.abs(value).toFixed(1)}%`

/** A change at a glance: cents below $1,000, then compact ("+$12.3K"). */
export const formatSignedCompactCurrency = (value: number | null | undefined) =>
  value == null || Math.abs(value) < 1000
    ? formatSignedCurrency(value)
    : `${value > 0 ? '+' : ''}${formatCompactCurrency(value)}`

export const formatCompactCurrency = (value: number) =>
  typographicMinus(compactCurrencyFormatter.format(value))

/** One amount when the bounds agree, otherwise the bounded span. */
export const formatCurrencyRange = ({ low, high }: { low: number; high: number }) =>
  low === high ? formatCurrency(low) : `${formatCurrency(low)}–${formatCurrency(high)}`

// A change carries its sign; no change carries none. Changes and returns keep two decimals.
export const formatPercent = (value: number | null | undefined) =>
  value == null ? '—' : `${value > 0 ? '+' : ''}${typographicMinus((value || 0).toFixed(2))}%`

/** A share of a whole (weight, allocation, utilization): one decimal, never signed. */
export const formatShare = (value: number | null | undefined, digits = 1) =>
  value == null ? '—' : `${Math.abs(value).toFixed(digits)}%`

export const formatSignedCurrency = (value: number | null | undefined) =>
  value == null ? '—' : `${value > 0 ? '+' : ''}${formatCurrency(value || 0)}`

export const formatUpdatedAt = (iso: string) => {
  const date = new Date(iso)
  return (
    date.getFullYear() === new Date().getFullYear()
      ? currentYearUpdatedAtFormatter
      : updatedAtFormatter
  ).format(date)
}

export const formatSecurityName = (name: string) => name.replace(/\s+class\b.*$/i, '').trim()

export const valueTone = (value: number | null | undefined) =>
  value == null || value === 0 ? 'muted' : value > 0 ? 'positive' : 'negative'

/** The tone of a change, where some figures (spending, fees) improve as they fall. */
export const changeTone = (
  value: number | null | undefined,
  favorable: 'increase' | 'decrease' = 'increase',
) => valueTone(value == null ? value : favorable === 'decrease' ? -value : value)

export const formatShares = (value: number | null | undefined) =>
  value?.toLocaleString('en-US', { maximumFractionDigits: 4 }) ?? '—'

/** Abbreviations banks print in capitals that should stay that way. */
const descriptorCodes = new Set([
  'ACH',
  'AMEX',
  'APY',
  'ARC',
  'ATM',
  'CCD',
  'CHK',
  'CORP',
  'CRD',
  'CTR',
  'DBT',
  'DDA',
  'DEP',
  'EFT',
  'ETF',
  'INC',
  'IRA',
  'LLC',
  'LTD',
  'MKT',
  'PMT',
  'POS',
  'PPD',
  'REF',
  'SAV',
  'TEL',
  'USA',
  'USD',
  'VISA',
  'WEB',
])

/**
 * Bank descriptors arrive shouted ("ACH RENT PAYMENT"). When most of a string's letters are
 * capitals, each all-caps word is cased as a name, except two-letter words (state and country
 * codes), known abbreviations, and words with no vowel, which read as codes. Display only: the
 * raw text is kept everywhere it is evidence.
 */
export function displayCase(value: string) {
  const letters = value.match(/\p{L}/gu) ?? []
  if (letters.length < 3) return value
  const capitals = letters.filter((letter) => letter === letter.toLocaleUpperCase('en-US')).length
  if (capitals / letters.length < 0.8) return value
  return value.replace(/\p{Lu}[\p{Lu}\p{M}'’]+/gu, (word) => {
    if (word.length <= 2 || descriptorCodes.has(word) || !/[AEIOUY]/.test(word.slice(1)))
      return word
    return word[0] + word.slice(1).toLocaleLowerCase('en-US')
  })
}

/** Display only: preserve complete names in the ledger, search, and exports. */
export function formatActivityName(value: string) {
  const words = displayCase(value).trim().split(/\s+/)
  return words.length > 5 ? `${words.slice(0, 5).join(' ')}…` : words.join(' ')
}

/** "1 account", "2 accounts": the count with the right form of its noun. */
export const countOf = (count: number, singular: string, plural = `${singular}s`) =>
  `${count.toLocaleString('en-US')} ${count === 1 ? singular : plural}`

/** The noun alone, for a count rendered separately. */
export const pluralize = (count: number, singular: string, plural = `${singular}s`) =>
  count === 1 ? singular : plural
