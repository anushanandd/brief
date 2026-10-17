import type { AnalyticsSearch } from './analytics'
import { formatCurrency, formatCurrencyRange, formatShortDay } from './format'
import type { BenefitHistory, BenefitPeriod, CardBenefits } from './schema'

type AmountRange = { low: number; high: number }

/**
 * The calendar year Benefits shows: a custom window that spans exactly one year, otherwise this
 * year. The page has one period, so the Money strip follows the same year.
 */
export function benefitYear(search: AnalyticsSearch, today: string) {
  const year = search.from?.slice(0, 4)
  return year && search.from === `${year}-01-01` && search.to === `${year}-12-31`
    ? Number(year)
    : Number(today.slice(0, 4))
}

/** The search that shows `year`: year to date for this year, the whole year before it. */
export function benefitYearSearch(
  search: AnalyticsSearch,
  year: number,
  today: string,
): AnalyticsSearch {
  return year === Number(today.slice(0, 4))
    ? { ...search, range: 'year', from: undefined, to: undefined }
    : { ...search, range: undefined, from: `${year}-01-01`, to: `${year}-12-31` }
}

const yearOf = (period: BenefitPeriod) => Number(period.start.slice(0, 4))

/** Years with periods, newest first; this year always. */
export function benefitYears(card: CardBenefits | undefined, today: string) {
  const years = new Set([
    Number(today.slice(0, 4)),
    ...(card?.history ?? []).flatMap(({ periods }) => periods.map(yearOf)),
  ])
  return [...years].toSorted((left, right) => right - left)
}

/** A benefit's periods in one year. */
export const periodsIn = (benefit: BenefitHistory, year: number) =>
  benefit.periods.filter((period) => yearOf(period) === year)

export type BenefitGridCell =
  | { kind: 'period'; period: BenefitPeriod; span: number }
  | { kind: 'gap'; span: number }

/** A row's twelve month columns: each period spans its months, and gaps fill the rest. */
export function gridCells(periods: BenefitPeriod[]): BenefitGridCell[] {
  const cells: BenefitGridCell[] = []
  let month = 0
  for (const period of periods.toSorted((left, right) => left.start.localeCompare(right.start))) {
    const first = Number(period.start.slice(5, 7)) - 1
    const last = Number(period.end.slice(5, 7)) - 1
    if (first > month) cells.push({ kind: 'gap', span: first - month })
    cells.push({ kind: 'period', period, span: last - first + 1 })
    month = last + 1
  }
  if (month < 12) cells.push({ kind: 'gap', span: 12 - month })
  return cells
}

/**
 * The rows a year shows: benefits that aren't hidden, and a retired one only while it has
 * credits that year.
 */
export function benefitRows(
  card: CardBenefits | undefined,
  year: number,
  hiddenBenefitIds: readonly string[],
) {
  return (card?.history ?? []).filter(
    (benefit) =>
      !hiddenBenefitIds.includes(benefit.id) &&
      periodsIn(benefit, year).length > 0 &&
      (!benefit.retired || periodsIn(benefit, year).some(({ credited }) => credited !== 0)),
  )
}

const nothing: AmountRange = { low: 0, high: 0 }
const emptyYear = { captured: 0, planned: 0, missed: nothing, left: nothing }

/** One benefit's year as Rust summed it; Uber Cash, estimated from charges, adds nothing. */
export const benefitYearTotals = (benefit: BenefitHistory | undefined, year: number) =>
  benefit?.years.find((entry) => entry.year === year) ?? emptyYear

/**
 * The card's year over the benefits shown, as Rust summed it, with the part of what it captured
 * that landed on purchases made for someone else: the allowance was used, but the saving went
 * to them.
 */
export const cardYearTotals = (card: Pick<CardBenefits, 'years'> | undefined, year: number) =>
  card?.years.find((entry) => entry.year === year) ?? { ...emptyYear, forOthers: 0 }

export const periodStateText: Record<BenefitPeriod['state'], string> = {
  full: 'Used in full',
  partial: 'Partly used',
  missed: 'Missed',
  open: 'Open',
  settling: 'Settling',
  future: 'Not started',
  'not-visible': 'Not on card',
  'likely-used': 'Likely used',
  'before-history': 'Before history',
  'not-tracked': 'No credit',
}

const planned = (period: BenefitPeriod) => {
  const amount = period.plannedTotal
  return amount > 0 ? ` · ${formatCurrency(amount)} estimated` : ''
}

/** One line for a period: what was credited, what it left, and what should still come. */
export function periodSummary(period: BenefitPeriod) {
  const range = formatCurrencyRange
  switch (period.state) {
    case 'full':
      return `${formatCurrency(period.credited)} credited`
    case 'partial':
      return `${formatCurrency(period.credited)} of ${formatCurrency(period.cap)} · ${range(period.unused!)} missed`
    case 'missed':
      return `${range(period.unused!)} missed`
    case 'open':
      return `${formatCurrency(period.credited)} credited · ${period.left ? `${range(period.left)} left · ` : ''}ends ${formatShortDay(period.end)}${planned(period)}`
    case 'settling':
      return `${formatCurrency(period.credited)} credited so far`
    case 'future':
      return `Opens ${formatShortDay(period.start)}${planned(period)}`
    case 'not-visible':
      return 'Spent in the Uber app'
    case 'likely-used':
      return 'An Uber charge suggests it ran out'
    case 'before-history':
      return period.credited ? `${formatCurrency(period.credited)} credited` : 'Before history'
    default:
      return 'No credit'
  }
}
