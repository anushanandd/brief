import { formatActivityName, formatCurrency, formatCurrencyRange } from './format'
import type { BenefitActivity, BenefitWindow } from './schema'

const dayLabel = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  timeZone: 'UTC',
})
const formatDay = (date: string) => dayLabel.format(new Date(`${date}T00:00:00Z`))

/** What became of a qualifying purchase's credit, as judged natively. */
export function purchaseCreditLabel(credit: BenefitActivity['credit']) {
  switch (credit?.state) {
    case 'credited':
      return `Credited ${formatCurrency(credit.amount)}`
    case 'expected':
      return `Credit due by ${formatDay(credit.by)}`
    case 'late':
      return `Late · allowed until ${formatDay(credit.by)}`
    case 'not-credited':
      return 'No credit'
    case 'over-allowance':
      return 'Over allowance'
    default:
      return null
  }
}

const isPostedCredit = ({ kind, pending }: BenefitActivity) =>
  !pending && (kind === 'credit' || kind === 'reversal')

/** `label` is the short form beside the merchant; `text` is the full note behind it. */
export type ActivityBenefitNote = { text: string; label: string; tone?: 'positive' | 'warning' }

/** The short form of a purchase's credit state, beside the merchant it describes. */
function purchaseCreditBadge(credit: BenefitActivity['credit']) {
  switch (credit?.state) {
    case 'credited':
      return 'Credited'
    case 'expected':
      return 'Credit due'
    case 'late':
      return 'Credit late'
    case 'not-credited':
      return 'No credit'
    case 'over-allowance':
      return 'Over allowance'
    default:
      return 'Benefit'
  }
}

/**
 * What a ledger row means for a card benefit: whether a purchase was credited, or which
 * purchase a credit offset.
 */
export function activityBenefitNote(
  item: BenefitActivity,
  paired?: { merchant: string; date: string },
): ActivityBenefitNote | null {
  const name = item.benefitName
  if (item.kind === 'purchase') {
    const label = purchaseCreditLabel(item.credit)
    if (!label) return null
    const tone =
      item.credit?.state === 'credited'
        ? 'positive'
        : item.credit?.state === 'late' || item.credit?.state === 'not-credited'
          ? 'warning'
          : undefined
    return { text: `${name} · ${label}`, label: purchaseCreditBadge(item.credit), tone }
  }
  if (item.pending) return null
  if (item.kind === 'credit')
    return {
      text: paired
        ? `${name} credit for ${formatActivityName(paired.merchant)} on ${formatDay(paired.date)}`
        : `${name} credit`,
      label: 'Benefit credit',
    }
  if (item.kind === 'reversal')
    return { text: `${name} credit reversed`, label: 'Credit reversed', tone: 'warning' }
  return null
}

/** The one-line state of a benefit window. */
export function benefitStatus(benefit: BenefitWindow): {
  text: string
  tone?: 'positive'
} {
  if (benefit.status === 'external')
    return benefit.estimatedCreditAmount
      ? { text: `Likely used · ${formatCurrency(benefit.estimatedCreditAmount)}` }
      : { text: 'Use not visible on the card' }
  if (benefit.snapshotBeforeWindow && benefit.cadence !== 'renewal')
    return { text: 'No saved activity for this period' }
  const credited = benefit.creditedAmount
  if (credited !== 0 || benefit.uncertainCreditAmount !== 0) {
    const used = benefit.remainingRange?.high === 0
    return {
      text: benefit.remainingRange
        ? `${formatCurrency(credited)} of ${formatCurrency(benefit.cap)} credited`
        : `${formatCurrency(credited)} ${credited < 0 ? 'net reversal' : 'credited'}`,
      tone: used ? 'positive' : undefined,
    }
  }
  if (benefit.activity.some(isPostedCredit)) return { text: `${formatCurrency(0)} net credit` }
  if (benefit.awaitingCreditAmount > 0)
    return { text: `${formatCurrency(benefit.awaitingCreditAmount)} credit expected` }
  if (benefit.status === 'matched') return { text: 'Purchase found · no credit yet' }
  return { text: 'No credit detected' }
}

/** The supporting line under a benefit's status: what is left and how sure that is. */
export function benefitNote(benefit: BenefitWindow) {
  if (benefit.status === 'external')
    return benefit.estimatedCreditAmount
      ? 'An Uber charge suggests it ran out'
      : 'Spent in the Uber app'
  if (benefit.cadence === 'renewal')
    return benefit.renewalCheck ? `Renews ~${benefit.renewalCheck}` : 'Renewal date unknown'
  const range = benefit.remainingRange
  if (range && range.high > 0 && benefit.historyFrom && !benefit.periodUncertain)
    return `${formatCurrency(range.high)} left · history from ${formatDay(benefit.historyFrom)}`
  if (range && benefit.periodUncertain)
    return `${formatCurrencyRange(range)} left · ${formatCurrency(
      Math.abs(benefit.uncertainCreditAmount),
    )} may be last period’s`
  if (benefit.unusableRemainder)
    return `Used · ${formatCurrency(benefit.unusableRemainder)} left over`
  if (range?.high === 0) return 'Fully credited'
  if (range)
    return benefit.awaitingCreditAmount > 0
      ? `${formatCurrency(range.low)} left · ${formatCurrency(benefit.awaitingCreditAmount)} expected`
      : `${formatCurrency(range.low)} left`
  if (benefit.recurring) return 'Credited when billed'
  if (benefit.cadence === 'purchase') return 'Credited this year'
  return benefit.windowLabel
}

/** A purchase in the window whose credit is overdue or never came, which suggests enrollment. */
export function benefitWarning(benefit: BenefitWindow) {
  const missing = benefit.activity.filter(
    ({ credit }) => credit?.state === 'late' || credit?.state === 'not-credited',
  )
  if (!missing.length) return null
  if (missing.length > 1) return `No credit for ${missing.length} purchases · check enrollment`
  const [purchase] = missing
  const name = formatActivityName(purchase.merchant)
  return purchase.credit?.state === 'late'
    ? `Credit for ${name} is late`
    : `No credit for ${name} · check enrollment`
}

/** How long credits take, and where that number comes from. */
export function creditLagNote(benefit: BenefitWindow) {
  return benefit.creditLagLearned
    ? `Within ${benefit.creditLagDays} days on this card`
    : `Usually ${benefit.creditLagDays} days; up to ${benefit.postingDelayDays}`
}
