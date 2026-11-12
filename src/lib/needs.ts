import { accountDisplayName } from './account-names'
import { addDays } from './dates'
import {
  displayCase,
  formatActivityName,
  formatCurrency,
  formatCurrencyRange,
  formatProsePercent,
  formatShare,
  formatShortDay,
} from './format'
import type { AttentionItem, FinanceSnapshot } from './schema'

export type NeedView = {
  id: string
  /** The day it concerns, for the row's leading column. */
  date?: string
  title: string
  detail?: string
  /** A personal amount, masked by screen privacy. */
  amount?: number
  link:
    | { to: '/accounts'; search: { account: string } }
    | { to: '/activities'; search: { open: string } }
    | { to: '/taxes'; search: Record<string, never> }
    | { to: '/spending'; search: Record<string, never> }
    | { to: '/holdings/retirement'; search: Record<string, never> }
    | { to: '/documents'; search: { document: string } }
    | { to: '/insights'; search: { view: 'rules' } }
    | {
        to: '/analytics'
        search: { chart: 'card-credits'; benefit?: string; review?: true }
      }
  urgent: boolean
  /** How sure Rust is of an inferred need, 0 to 1; a need read from records has none. */
  confidence?: number
  /** The transactions an inferred need read, which Activity lists. */
  evidence?: string[]
}

/** How sure, in words a row can carry beside its detail. */
export const confidenceLabel = (confidence: number) => `${formatShare(confidence * 100, 0)} sure`

export type InferredNeed = Extract<AttentionItem, { confidence: number }>

/**
 * The transactions each inferred need is about, so its row and inspector can carry it: every
 * fee a fees need counts, and otherwise the one transaction it names. The rest of its evidence
 * is history the inspector cites, not something to look at again.
 */
export function inferredNeedSubjects(needs: readonly AttentionItem[]) {
  const subjects = new Map<string, InferredNeed>()
  for (const need of needs) {
    if (!('confidence' in need)) continue
    for (const id of need.kind === 'fees-charged' ? need.evidence : [need.transactionId])
      if (!subjects.has(id)) subjects.set(id, need)
  }
  return subjects
}

const inDays = (days: number) =>
  days === 0
    ? 'today'
    : days === 1
      ? 'tomorrow'
      : days < 0
        ? `${-days} days ago`
        : `in ${days} days`

/** Words for one need Rust found. Rust decides what needs action; this only phrases it. */
export function needView(need: AttentionItem, data: FinanceSnapshot): NeedView {
  const names = data.settings?.accountNames ?? {}
  const accountName = (id: string) =>
    accountDisplayName(
      id,
      data.accounts.find((account) => account.id === id)?.name ?? 'Card',
      names,
    )
  const urgent = need.severity === 'urgent'
  // Benefits opens on the spending card; another tracked card's needs open its account.
  const benefitsPage = (
    accountId: string,
    search: { benefit?: string; review?: true },
  ): NeedView['link'] =>
    accountId === data.insights?.benefits?.accountId
      ? { to: '/analytics', search: { chart: 'card-credits', ...search } }
      : { to: '/accounts', search: { account: accountId } }
  switch (need.kind) {
    case 'payment-due':
      return {
        id: need.id,
        date: need.due,
        title: need.overdue
          ? `${accountName(need.accountId)} payment is overdue`
          : `${accountName(need.accountId)} payment due ${inDays(need.days)}`,
        detail: need.minimumPayment != null ? 'Minimum due' : undefined,
        amount: need.minimumPayment ?? undefined,
        link: { to: '/accounts', search: { account: need.accountId } },
        urgent,
      }
    case 'coverage-short':
      return {
        id: need.id,
        title: 'Bills exceed cash',
        detail: 'Next 30 days · shortfall',
        amount: -need.shortfall,
        link: { to: '/spending', search: {} },
        urgent,
      }
    case 'cash-low':
      return {
        id: need.id,
        date: need.date,
        title: `Cash may run short by ${formatShortDay(need.date)}`,
        detail: 'Expected low',
        amount: need.expected,
        link: { to: '/spending', search: {} },
        urgent,
      }
    case 'benefit-review':
      return {
        id: need.id,
        title: `${need.count} benefit ${need.count === 1 ? 'match' : 'matches'} to review`,
        detail: accountName(need.accountId),
        link: benefitsPage(need.accountId, { review: true }),
        urgent,
      }
    case 'credit-expiring':
      return {
        id: need.id,
        date: need.windowEnd,
        title: `${need.name} credit ends ${inDays(need.days)}`,
        detail: `${accountName(need.accountId)} · unused`,
        amount: need.remainingHigh,
        link: benefitsPage(need.accountId, { benefit: need.benefitId }),
        urgent,
      }
    case 'utilization-high':
      return {
        id: need.id,
        date: need.closesOn,
        title: `${accountName(need.accountId)} closes ${inDays(need.days)}`,
        detail: `${formatShare(need.utilization, 0)} of limit used · balance below · pay before it closes`,
        amount: -need.balance,
        link: { to: '/accounts', search: { account: need.accountId } },
        urgent,
      }
    case 'possible-duplicate':
      return {
        id: need.id,
        date: need.date,
        title: `Possible duplicate at ${formatActivityName(need.merchant)}`,
        detail: `${accountName(need.accountId)} · each charge`,
        amount: need.amount,
        link: { to: '/activities', search: { open: need.transactionIds[1] } },
        urgent,
      }
    case 'wash-window':
      return {
        id: need.id,
        date: need.until,
        title: `Don’t buy ${need.ticker} until ${formatShortDay(addDays(need.until, 1))}`,
        detail: `Keeps the ${formatShortDay(need.soldOn)} loss`,
        amount: need.loss,
        link: { to: '/taxes', search: {} },
        urgent,
      }
    case 'missed-bill':
      return {
        id: need.id,
        date: need.expected,
        title:
          need.event === 'income' || need.event === 'credit'
            ? `${formatActivityName(need.title)} hasn’t arrived`
            : `${formatActivityName(need.title)} hasn’t posted`,
        detail: `Due ${inDays(-need.days)}${need.accountId ? ` · ${accountName(need.accountId)}` : ''} · mark its last charge reviewed to dismiss`,
        amount: need.amount,
        link: { to: '/activities', search: { open: need.sourceTransactionId } },
        urgent,
      }
    case 'contributions-to-confirm':
      return {
        id: need.id,
        title: `${need.count === 1 ? 'A deposit' : `${need.count} deposits`} into ${accountName(need.accountId)} to place`,
        detail: 'Which year it counts for, or a rollover',
        amount: need.amount,
        link: { to: '/holdings/retirement', search: {} },
        urgent,
      }
    case 'contribution-limit': {
      const plan = need.plan === 'ira' ? 'IRA' : '401(k)'
      return {
        id: need.id,
        title: need.onPace
          ? `${plan} deferrals on pace to pass the ${need.taxYear} limit`
          : `${need.taxYear} ${plan} contributions are over the limit`,
        detail: need.onPace
          ? `Limit ${formatCurrency(need.limit)} · lower the paycheck amount`
          : `Limit ${formatCurrency(need.limit)} · withdraw the excess by the tax deadline`,
        amount: need.amount,
        link: { to: '/holdings/retirement', search: {} },
        urgent,
      }
    }
    case 'pay-missing':
      return {
        id: need.id,
        date: need.date,
        title: `Pay from ${displayCase(need.employer)} hasn’t arrived`,
        detail: need.accountId
          ? `Statement says ${accountName(need.accountId)}`
          : 'The pay statement shows a deposit',
        amount: need.amount,
        link: { to: '/documents', search: { document: need.documentId } },
        urgent,
      }
    case 'w2-differs':
      return {
        id: need.id,
        title: `${displayCase(need.employer)}’s ${need.taxYear} W-2 differs from your pay`,
        detail: `${need.boxes} ${need.boxes === 1 ? 'box' : 'boxes'} to check before you file`,
        link: { to: '/documents', search: { document: need.documentId } },
        urgent,
      }
    case 'idle-cash':
      return {
        id: need.id,
        title: `${accountName(need.accountId)} earns ${need.annualRate == null ? 'little' : formatProsePercent(need.annualRate)}`,
        detail: `${accountName(need.betterAccountId)} pays ${formatProsePercent(need.betterRate)} · balance below`,
        amount: need.balance,
        link: { to: '/accounts', search: { account: need.accountId } },
        urgent,
      }
    // A rule's own words; its figures are the rule's published terms, not your amounts.
    case 'program-note':
      return {
        id: need.id,
        title: need.says,
        detail:
          data.insights?.evidence.programs.find((program) => program.id === need.program)?.name ??
          'A rule Brief knows',
        link: { to: '/insights', search: { view: 'rules' } },
        urgent,
      }
    // Inferences: each says how sure it is and opens the transaction it is about.
    case 'new-series':
      return {
        id: need.id,
        date: need.firstSeen,
        title: `New ${need.frequency.toLowerCase()} ${need.event === 'income' ? 'deposit' : 'charge'} from ${formatActivityName(need.title)}`,
        detail: `First seen ${formatShortDay(need.firstSeen)}${need.accountId ? ` · ${accountName(need.accountId)}` : ''}`,
        amount: need.amount,
        link: { to: '/activities', search: { open: need.transactionId } },
        urgent,
        confidence: need.confidence,
        evidence: need.evidence,
      }
    case 'price-changed':
      return {
        id: need.id,
        date: need.changedOn,
        title: `${formatActivityName(need.title)} ${Math.abs(need.latest) > Math.abs(need.previous) ? 'raised' : 'lowered'} its price`,
        detail: `Was ${formatCurrency(Math.abs(need.previous))}`,
        amount: need.latest,
        link: { to: '/activities', search: { open: need.transactionId } },
        urgent,
        confidence: need.confidence,
        evidence: need.evidence,
      }
    case 'fees-charged':
      return {
        id: need.id,
        date: need.since,
        title: `${need.count === 1 ? 'A fee' : `${need.count} fees`} charged since ${formatShortDay(need.since)}`,
        detail: 'Bank fees, card interest, and foreign transaction fees',
        amount: need.amount,
        link: { to: '/activities', search: { open: need.transactionId } },
        urgent,
        confidence: need.confidence,
        evidence: need.evidence,
      }
    case 'pay-differs':
      return {
        id: need.id,
        date: need.date,
        title: `${formatActivityName(need.title)} paid ${need.amount > need.high ? 'more' : 'less'} than usual`,
        detail: `Usually ${formatCurrencyRange(need)}`,
        amount: need.amount,
        link: { to: '/activities', search: { open: need.transactionId } },
        urgent,
        confidence: need.confidence,
        evidence: need.evidence,
      }
    case 'unusual-charge':
      return {
        id: need.id,
        title: `Unusual charge at ${formatActivityName(need.merchant)}`,
        detail: `Usually ${formatCurrency(Math.abs(need.usual))} · ${need.observations} earlier charges`,
        amount: need.amount,
        link: { to: '/activities', search: { open: need.transactionId } },
        urgent,
        confidence: need.confidence,
        evidence: need.evidence,
      }
    default: {
      const unknown: never = need
      throw new Error(`Unknown need ${JSON.stringify(unknown)}`)
    }
  }
}
