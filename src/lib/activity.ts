import { accountDisplayName, type AccountDisplayNames } from './account-name-preferences'
import { formatCurrency } from './format'
import { transactionLogoUrl, transactionMarkKind } from './logos'
import type { FinanceSnapshot, Transaction } from './schema'
import { isSpendingTransaction, transactionDateKey } from './spending'
import type { TransactionMarkKind } from './transaction-kind'

export type ActivityItem = {
  id: string
  kind: 'spending' | 'transaction' | 'income' | 'credit' | 'transfer' | 'trade'
  mark?: TransactionMarkKind
  accountId?: string
  account?: string
  category: string
  title: string
  detail: string
  date: string
  location?: Transaction['location']
  paymentChannel?: string
  amount: number
  estimatedRealizedGain?: number | null
  pending?: boolean
  description?: string
  logoUrl?: string
  website?: string
}

export type ActivitySortColumn =
  | 'description'
  | 'account'
  | 'category'
  | 'location'
  | 'method'
  | 'date'
  | 'amount'
export type ActivitySort = { column: ActivitySortColumn; direction: 'asc' | 'desc' }

export function activityLocationText(location?: ActivityItem['location']) {
  if (!location) return ''
  return [
    location.address,
    [location.city, location.region].filter(Boolean).join(', '),
    location.postalCode,
    location.country,
  ]
    .filter(Boolean)
    .join(' · ')
}

export function activityLocationLabel(location?: ActivityItem['location']) {
  const place = [location?.city, location?.region, location?.country].filter(Boolean).join(', ')
  return place || location?.postalCode || (location?.address ? 'Address' : '—')
}

export function paymentChannelLabel(channel?: string) {
  const value = channel?.trim().replaceAll(/[_-]+/g, ' ')
  return value ? value[0].toUpperCase() + value.slice(1).toLowerCase() : ''
}

export function activityMethod(activity: ActivityItem) {
  return activity.kind === 'trade' || activity.accountId?.startsWith('snaptrade:')
    ? 'Brokerage'
    : paymentChannelLabel(activity.paymentChannel)
}

export function activityMatchesSearch(activity: ActivityItem, query: string) {
  const text = query.trim().toLocaleLowerCase()
  if (!text) return true
  const fields = {
    description: `${activity.title} ${activity.description ?? ''}`,
    account: activity.account ?? '',
    category: activity.category,
    method: activityMethod(activity),
    location: activityLocationText(activity.location),
    amount: `${activity.amount} ${formatCurrency(activity.amount)}`,
    date: activity.date,
  }
  const scoped = /^(description|account|category|method|location|amount|date):\s*(.*)$/.exec(text)
  if (scoped && !scoped[2].trim()) return false
  const source = scoped
    ? (Object.entries(fields).find(([name]) => name === scoped[1])?.[1] ?? '')
    : `${Object.values(fields).join(' ')} ${activity.detail}`
  const terms = (scoped ? scoped[2] : text).split(/\s+/).filter(Boolean)
  return terms.every((term) => source.toLocaleLowerCase().includes(term))
}

export function linkedAccountIds(
  accountIds: readonly (string | undefined)[],
  accountLinks: Readonly<Record<string, string>> = {},
) {
  const ids = new Set(accountIds.filter((id): id is string => Boolean(id)))
  for (const [plaidId, snaptradeId] of Object.entries(accountLinks)) {
    if (ids.has(plaidId) || ids.has(snaptradeId)) {
      ids.add(plaidId)
      ids.add(snaptradeId)
    }
  }
  return ids
}

export function sortActivities(
  activities: ActivityItem[],
  referenceIso: string,
  sort: ActivitySort,
) {
  const direction = sort.direction === 'asc' ? 1 : -1
  const value = (activity: ActivityItem) =>
    sort.column === 'description'
      ? activity.title
      : sort.column === 'account'
        ? (activity.account ?? '')
        : sort.column === 'location'
          ? [
              activity.location?.city,
              activity.location?.region,
              activity.location?.country,
              activity.location?.postalCode,
              activity.location?.address,
            ]
              .filter(Boolean)
              .join(' · ')
          : sort.column === 'method'
            ? activityMethod(activity)
            : sort.column === 'date'
              ? transactionDateKey(activity.date, referenceIso)
              : activity.category
  return activities.toSorted((left, right) => {
    if (sort.column === 'amount') return direction * (left.amount - right.amount)
    if (sort.column === 'date') return direction * value(left).localeCompare(value(right))
    return direction * value(left).localeCompare(value(right), undefined, { sensitivity: 'base' })
  })
}

export function buildActivities(
  data: Pick<FinanceSnapshot, 'transactions' | 'trades' | 'updatedAt'> & Partial<FinanceSnapshot>,
  accountDisplayNames: AccountDisplayNames = {},
  externalLogosEnabled = false,
): ActivityItem[] {
  const transactions = data.transactions.map((transaction): ActivityItem => {
    const mark = transactionMarkKind(transaction)
    const isCredit = transaction.classification.credit
    const account = accountDisplayName(
      transaction.accountId,
      transaction.account,
      accountDisplayNames,
    )
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
      mark,
      accountId: transaction.accountId,
      account,
      category,
      title: transaction.merchant,
      detail: `${account} · ${category}`,
      date: transaction.date,
      location: transaction.location,
      paymentChannel: transaction.paymentChannel,
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
    const account = accountDisplayName(trade.accountId, trade.account, accountDisplayNames)
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
      account,
      category: 'Trade',
      title: `${verb} ${trade.ticker ?? 'security'}`,
      detail: [
        account,
        trade.units != null ? `${trade.units} shares` : trade.description,
        trade.estimatedRealizedGain != null
          ? `Estimated FIFO P/L ${formatCurrency(trade.estimatedRealizedGain)}`
          : undefined,
      ]
        .filter(Boolean)
        .join(' · '),
      date: trade.date,
      amount: trade.amount,
      estimatedRealizedGain: trade.estimatedRealizedGain,
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
