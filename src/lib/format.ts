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
/*
 * Dates and times, all in US English. Two kinds, never mixed:
 * - A calendar day from Rust (`YYYY-MM-DD`) names a day, not an instant, so it formats in UTC
 *   and reads the same in every time zone (`formatCalendarDate` and the named days below).
 * - An instant (an ISO time, epoch milliseconds, or a Date) reads in the user's own time zone
 *   (`formatInstant`), except market session times, which are Eastern and say so
 *   (`formatMarketTime`).
 */
const dateFormatters = new Map<string, Intl.DateTimeFormat>()
function dateFormatter(options: Intl.DateTimeFormatOptions) {
  const key = JSON.stringify(options)
  let formatter = dateFormatters.get(key)
  if (!formatter) {
    formatter = new Intl.DateTimeFormat('en-US', options)
    dateFormatters.set(key, formatter)
  }
  return formatter
}

/** A calendar day, `YYYY-MM-DD`, in any style; it never shifts with the time zone. */
export const formatCalendarDate = (date: string, options: Intl.DateTimeFormatOptions) =>
  dateFormatter({ ...options, timeZone: 'UTC' }).format(new Date(`${date}T00:00:00Z`))
/** A calendar day, `YYYY-MM-DD`, as "Sep 5". */
export const formatShortDay = (date: string) =>
  formatCalendarDate(date, { month: 'short', day: 'numeric' })
/** A calendar day, `YYYY-MM-DD`, as "Sep 5, 2026". */
export const formatDay = (date: string) =>
  formatCalendarDate(date, { month: 'short', day: 'numeric', year: 'numeric' })
/** A calendar day's month, `YYYY-MM-DD`, as "Sep 2026". */
export const formatMonthYear = (date: string) =>
  formatCalendarDate(date, { month: 'short', year: 'numeric' })

/** An instant in the user's time zone; `options.timeZone` names another. */
export const formatInstant = (
  instant: string | number | Date,
  options: Intl.DateTimeFormatOptions = {
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
  },
) => dateFormatter(options).format(new Date(instant))

/** A market session time, Eastern and labeled so: "9:30 AM ET". */
export const formatMarketTime = (instant: string | number | Date) =>
  `${formatInstant(instant, { hour: 'numeric', minute: '2-digit', timeZone: 'America/New_York' })} ET`

/** A rate as given (an APR, a yield): two decimals, never signed. */
export const formatRate = (value: number) => `${value.toFixed(2)}%`
/**
 * Negative figures carry a true minus (U+2212): in tabular figures it shares the plus sign's
 * width, so signed columns line up. Exports and copies translate it back to a hyphen.
 */
export const MINUS = '−'
const typographicMinus = (text: string) => text.replace(/^-/, MINUS)
/** The ASCII form, for clipboards and files that parse numbers. */
export const asciiMinus = (text: string) => text.replaceAll(MINUS, '-')

/**
 * Screen privacy (⌘H) masks personal amounts where they're formatted, so every amount hides,
 * in text, labels, and chart axes alike, without each view opting in. The same placeholder
 * stands for every value, so nothing about its size shows. Public market prices format with
 * `formatPublicCurrency` and stay visible.
 */
let amountsMasked = false
export const HIDDEN_AMOUNT = '$••••'
const maskListeners = new Set<() => void>()
/**
 * Set by the app shell as it renders, so everything rendering with it formats the same way.
 * The shell then calls `announceAmountsHidden`, which re-renders the views that read the mask
 * through `useAmountsHidden` but didn't render with the shell: memoized lists and every page.
 */
export function setAmountsHidden(hidden: boolean) {
  amountsMasked = hidden
}
export const amountsHidden = () => amountsMasked
export function subscribeAmountsHidden(listener: () => void) {
  maskListeners.add(listener)
  return () => void maskListeners.delete(listener)
}
export function announceAmountsHidden() {
  for (const listener of maskListeners) listener()
}

/** A public market price, which screen privacy leaves visible. */
export const formatPublicCurrency = (value: number | null | undefined) =>
  value == null ? '—' : typographicMinus(currencyFormatter.format(value))

export const formatCurrency = (value: number | null | undefined) =>
  value == null ? '—' : amountsMasked ? HIDDEN_AMOUNT : formatPublicCurrency(value)

/**
 * Sentences round: whole dollars and one-decimal percents. Figures in rows, metrics, and
 * selectors keep cents and two decimals. Both use the same minus.
 */
export const formatProseCurrency = (value: number) =>
  amountsMasked ? HIDDEN_AMOUNT : typographicMinus(wholeCurrencyFormatter.format(value))
export const formatProsePercent = (value: number) => `${Math.abs(value).toFixed(1)}%`

/** A change at a glance: cents below $1,000, then compact ("+$12.3K"). */
export const formatSignedCompactCurrency = (value: number | null | undefined) =>
  value == null || Math.abs(value) < 1000
    ? formatSignedCurrency(value)
    : amountsMasked
      ? HIDDEN_AMOUNT
      : `${value > 0 ? '+' : ''}${formatCompactCurrency(value)}`

export const formatCompactCurrency = (value: number) =>
  amountsMasked ? HIDDEN_AMOUNT : typographicMinus(compactCurrencyFormatter.format(value))

/** One amount when the bounds agree, otherwise the bounded span. */
export const formatCurrencyRange = ({ low, high }: { low: number; high: number }) =>
  low === high || amountsMasked
    ? formatCurrency(low)
    : `${formatCurrency(low)}–${formatCurrency(high)}`

// A change carries its sign; no change carries none. Changes and returns keep two decimals.
export const formatPercent = (value: number | null | undefined) =>
  value == null ? '—' : `${value > 0 ? '+' : ''}${typographicMinus((value || 0).toFixed(2))}%`

/** A share of a whole (weight, allocation, utilization): one decimal, never signed. */
export const formatShare = (value: number | null | undefined, digits = 1) =>
  value == null ? '—' : `${Math.abs(value).toFixed(digits)}%`

export const formatSignedCurrency = (value: number | null | undefined) =>
  value == null
    ? '—'
    : amountsMasked
      ? HIDDEN_AMOUNT
      : `${value > 0 ? '+' : ''}${formatCurrency(value || 0)}`

/**
 * A personal amount in any currency (an ISO 4217 code), masked like dollars and signed as
 * `formatCurrency` signs them. A code Intl doesn't know follows the figure.
 */
export function formatCurrencyIn(value: number, currency: string) {
  if (currency === 'USD') return formatCurrency(value)
  if (amountsMasked) return HIDDEN_AMOUNT
  try {
    return typographicMinus(
      new Intl.NumberFormat('en-US', { style: 'currency', currency }).format(value),
    )
  } catch {
    return typographicMinus(`${value.toFixed(2)} ${currency}`)
  }
}

/** When something was saved or observed: "Sep 5, 4:02 PM", with the year when it isn't this one. */
export const formatUpdatedAt = (iso: string | number) => {
  const date = new Date(iso)
  return formatInstant(date, {
    month: 'short',
    day: 'numeric',
    ...(date.getFullYear() === new Date().getFullYear() ? {} : { year: 'numeric' }),
    hour: 'numeric',
    minute: '2-digit',
  })
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
