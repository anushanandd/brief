import { accountDisplayName } from './account-names'
import { addDays } from './dates'
import { formatActivityName, formatShortDay } from './format'
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
  urgent: boolean
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
    case 'credit-expiring':
      return {
        id: need.id,
        date: need.windowEnd,
        title: `${need.name} credit ends ${inDays(need.days)}`,
        detail: `${accountName(need.accountId)} · unused`,
        amount: need.remainingHigh,
        link: { to: '/accounts', search: { account: need.accountId } },
        urgent,
      }
    case 'utilization-high':
      return {
        id: need.id,
        date: need.closesOn,
        title: `${accountName(need.accountId)} closes ${inDays(need.days)}`,
        detail: `${need.utilization.toFixed(0)}% of limit used · balance below · pay before it closes`,
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
    default: {
      const unknown: never = need
      throw new Error(`Unknown need ${JSON.stringify(unknown)}`)
    }
  }
}
