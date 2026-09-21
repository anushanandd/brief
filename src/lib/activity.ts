import { accountDisplayName, type AccountDisplayNames } from './account-name-preferences'
import { formatCurrency } from './format'
import { transactionLogoUrl, transactionMarkKind } from './logos'
import type { FinanceSnapshot } from './schema'
import { isSpendingTransaction, transactionDateKey } from './spending'

export type ActivityItem = {
  id: string
  kind: 'spending' | 'transaction' | 'income' | 'credit' | 'transfer' | 'trade'
  accountId?: string
  category: string
  title: string
  detail: string
  date: string
  amount: number
  pending?: boolean
  description?: string
  logoUrl?: string
  website?: string
}

export function buildActivities(
  data: Pick<FinanceSnapshot, 'transactions' | 'trades' | 'updatedAt'> & Partial<FinanceSnapshot>,
  accountDisplayNames: AccountDisplayNames = {},
  externalLogosEnabled = false,
): ActivityItem[] {
  const transactions = data.transactions.map((transaction): ActivityItem => {
    const mark = transactionMarkKind(transaction)
    const isCredit = transaction.classification.credit
    const category = isCredit ? 'Credit' : transaction.category || 'Other'
    const kind =
      mark === 'transfer'
        ? 'transfer'
        : isCredit
          ? 'credit'
          : mark === 'income' || mark === 'interest' || mark === 'dividend'
            ? 'income'
            : isSpendingTransaction(transaction)
              ? 'spending'
              : 'transaction'
    return {
      id: `spending:${transaction.id}`,
      kind,
      accountId: transaction.accountId,
      category,
      title: transaction.merchant,
      detail: `${accountDisplayName(transaction.accountId, transaction.account, accountDisplayNames)} · ${category}`,
      date: transaction.date,
      amount: transaction.amount,
      pending: transaction.pending,
      description:
        transaction.description && transaction.description !== transaction.merchant
          ? transaction.description
          : undefined,
      website: transaction.website,
      ...((kind === 'spending' || kind === 'transaction') && {
        logoUrl: transactionLogoUrl(transaction, externalLogosEnabled),
      }),
    }
  })
  const trades = data.trades.map((trade): ActivityItem => {
    const type = trade.type.toLocaleUpperCase()
    const verb = type.includes('BUY')
      ? 'Bought'
      : type.includes('SELL')
        ? 'Sold'
        : type.includes('REINVEST')
          ? 'Reinvested'
          : 'Traded'
    return {
      id: `trade:${trade.id}`,
      kind: 'trade',
      accountId: trade.accountId,
      category: 'Trade',
      title: `${verb} ${trade.ticker ?? 'security'}`,
      detail: [
        accountDisplayName(trade.accountId, trade.account, accountDisplayNames),
        trade.units != null ? `${trade.units} shares` : trade.description,
        trade.estimatedRealizedGain != null
          ? `Estimated FIFO P/L ${formatCurrency(trade.estimatedRealizedGain)}`
          : undefined,
      ]
        .filter(Boolean)
        .join(' · '),
      date: trade.date,
      amount: trade.amount,
    }
  })
  return [...transactions, ...trades].toSorted(
    (left, right) =>
      transactionDateKey(right.date, data.updatedAt).localeCompare(
        transactionDateKey(left.date, data.updatedAt),
      ) || right.id.localeCompare(left.id),
  )
}

const monthFormatter = new Intl.DateTimeFormat('en-US', { month: 'long', timeZone: 'UTC' })
const monthYearFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
})

export function activityDateGroup(date: string, referenceIso: string) {
  const activityDay = transactionDateKey(date, referenceIso)
  const referenceDay = transactionDateKey(referenceIso, referenceIso)
  const daysAgo = Math.round(
    (Date.parse(`${referenceDay}T00:00:00Z`) - Date.parse(`${activityDay}T00:00:00Z`)) / 86_400_000,
  )

  if (daysAgo < 0) return 'Upcoming'
  if (daysAgo === 0) return 'Today'
  if (daysAgo === 1) return 'Yesterday'
  if (!activityDay) return 'Unknown date'
  if (daysAgo <= 7 && activityDay.slice(0, 7) === referenceDay.slice(0, 7)) return 'Last week'
  const formatter =
    activityDay.slice(0, 4) === referenceDay.slice(0, 4) ? monthFormatter : monthYearFormatter
  return formatter.format(new Date(`${activityDay}T00:00:00Z`))
}

export function groupActivitiesByDate(activities: ActivityItem[], referenceIso: string) {
  const groups: Array<{ label: string; activities: ActivityItem[] }> = []
  for (const activity of activities) {
    const label = activityDateGroup(activity.date, referenceIso)
    const last = groups.at(-1)
    if (last?.label === label) last.activities.push(activity)
    else groups.push({ label, activities: [activity] })
  }
  return groups
}
