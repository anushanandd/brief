import type { AnalyticsSearch } from './analytics'
import { formatCurrency, formatCurrencyRange, formatShortDay } from './format'
import type { BenefitActivity, BenefitHistory, BenefitPeriod, CardBenefits } from './schema'

type AmountRange = { low: number; high: number }

const cents = (value: number) => Math.round(value * 100)
const sum = (values: number[]) => values.reduce((total, value) => total + cents(value), 0) / 100

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

/** Credits a period should still receive, summed in cents. */
export const plannedAmount = (period: BenefitPeriod) =>
  sum((period.planned ?? []).map(({ amount }) => amount))

/** Totals over the year's periods, summed in cents. Uber Cash, estimated from charges, adds nothing. */
export function benefitYearTotals(benefits: BenefitHistory[], year: number) {
  const periods = benefits
    .filter(({ external }) => !external)
    .flatMap((benefit) => periodsIn(benefit, year))
  const missed = periods.flatMap(({ unused }) => (unused ? [unused] : []))
  const open = periods.flatMap(({ left }) => (left ? [left] : []))
  return {
    captured: sum(periods.map(({ credited }) => credited)),
    /** Credits still to come from waiting purchases and expected charges. */
    planned: sum(periods.map(plannedAmount)),
    missed: {
      low: sum(missed.map(({ low }) => low)),
      high: sum(missed.map(({ high }) => high)),
    } satisfies AmountRange,
    left: {
      low: sum(open.map(({ low }) => low)),
      high: sum(open.map(({ high }) => high)),
    } satisfies AmountRange,
  }
}

/**
 * The share of the year's captured credits that landed on purchases made for someone else: the
 * allowance was used, but the saving went to them. Summed from the same periods as
 * `benefitYearTotals`, so what is yours and theirs add up to what was captured.
 */
export function capturedForOthers(
  benefits: BenefitHistory[],
  year: number,
  activity: ReadonlyMap<string, BenefitActivity>,
  others: ReadonlySet<string>,
) {
  return sum(
    benefits
      .filter(({ external }) => !external)
      .flatMap((benefit) => periodsIn(benefit, year))
      .flatMap(({ activityIds }) => activityIds)
      .flatMap((id) => {
        const item = activity.get(id)
        return item &&
          others.has(id) &&
          !item.pending &&
          (item.kind === 'credit' || item.kind === 'reversal')
          ? [item.amount]
          : []
      }),
  )
}

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
  const amount = plannedAmount(period)
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
