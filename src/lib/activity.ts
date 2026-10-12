import { accountDisplayName, type AccountDisplayNames } from './account-names'
import { activityBenefitNote, type ActivityBenefitNote } from './benefit-copy'
import { daysBetween } from './dates'
import { asciiMinus, formatCurrency } from './format'
import { stockLogoUrl, transactionLogoUrl } from './logos'
import { cents } from './money'
import type { FinanceSnapshot, Transaction } from './schema'
import { transactionDateKey } from './spending'
import {
  isSpendingTransaction,
  transactionMarkKind,
  type TransactionMarkKind,
} from './transaction-kind'

export type ActivityItem = {
  id: string
  kind: 'spending' | 'transaction' | 'income' | 'credit' | 'transfer' | 'trade'
  mark?: TransactionMarkKind
  accountId?: string
  account?: string
  category: string
  /** Plaid's specific label within the category, such as Coffee within Dining. */
  subcategory?: string
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
  /** What the row means for a card benefit, from the native benefit rules. */
  benefit?: ActivityBenefitNote
  tags?: string[]
  /** The categories a split transaction is divided between. */
  splitCategories?: string[]
  /** Whether a transaction is someone else's money; absent on trades, which can't be. */
  other?: boolean
  /** A second look Rust suggests: a possible duplicate or an unusual amount. */
  flag?: 'duplicate' | 'unusual'
}

/** Whether an activity belongs to a category, counting each part of a split. */
export const activityInCategory = (activity: ActivityItem, category: string) =>
  (activity.category || 'Other') === category ||
  Boolean(activity.splitCategories?.includes(category))

export type ActivitySortColumn =
  | 'description'
  | 'account'
  | 'category'
  | 'owner'
  | 'location'
  | 'method'
  | 'date'
  | 'amount'
export type ActivitySort = { column: ActivitySortColumn; direction: 'asc' | 'desc' }
/** A column whose cell narrows Activity to its value. */
export type ActivityFilterColumn = Exclude<ActivitySortColumn, 'amount' | 'owner'>

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

const domesticCountries = new Set(['US', 'USA', 'UNITED STATES'])

/**
 * Where a purchase happened, in one place: a city when the bank reports one, otherwise how it was
 * paid. The second line adds the street, the channel, or the merchant's site.
 */
export function activityPlace(activity: ActivityItem): {
  kind: 'place' | 'online' | 'store' | 'brokerage' | 'none'
  primary: string
  secondary: string
} {
  const location = activity.location
  const method = activityMethod(activity)
  const country =
    location?.country && !domesticCountries.has(location.country.toUpperCase())
      ? location.country
      : ''
  const place =
    [[location?.city, location?.region].filter(Boolean).join(', '), country]
      .filter(Boolean)
      .join(' · ') || location?.postalCode
  if (place || location?.address)
    return {
      kind: 'place',
      primary: place || location!.address!,
      secondary: (place && location?.address) || (method === 'Other' ? '' : method),
    }
  if (method === 'Online') {
    let host = ''
    try {
      host = activity.website ? new URL(activity.website).hostname.replace(/^www\./, '') : ''
    } catch {
      host = ''
    }
    return { kind: 'online', primary: 'Online', secondary: host }
  }
  if (method === 'In store') return { kind: 'store', primary: 'In store', secondary: '' }
  if (method === 'Brokerage') return { kind: 'brokerage', primary: 'Brokerage', secondary: '' }
  return { kind: 'none', primary: '—', secondary: '' }
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
    category: `${activity.category} ${activity.subcategory ?? ''}`,
    tag: activity.tags?.join(' ') ?? '',
    method: activityMethod(activity),
    location: activityLocationText(activity.location),
    amount: `${activity.amount} ${formatCurrency(activity.amount)} ${asciiMinus(formatCurrency(activity.amount))}`,
    date: activity.date,
  }
  const scoped = /^(description|account|category|tag|method|location|amount|date):\s*(.*)$/.exec(
    text,
  )
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
        : sort.column === 'owner'
          ? String(Boolean(activity.other))
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
  const benefitActivity = new Map(
    (data.insights?.benefits?.activity ?? []).map((item) => [item.id, item]),
  )
  const transactionsById = new Map(data.transactions.map((item) => [item.id, item]))
  const duplicates = new Set(
    data.insights?.anomalies.duplicates.flatMap(({ transactionIds }) => transactionIds),
  )
  const unusual = new Set(
    data.insights?.anomalies.unusual.map(({ transactionId }) => transactionId),
  )
  const flag = (id: string) =>
    duplicates.has(id) ? ('duplicate' as const) : unusual.has(id) ? ('unusual' as const) : undefined
  const benefitNote = (id: string) => {
    const item = benefitActivity.get(id)
    if (!item) return undefined
    const paired = item.pairedId
      ? (benefitActivity.get(item.pairedId) ?? transactionsById.get(item.pairedId))
      : undefined
    return activityBenefitNote(item, paired) ?? undefined
  }
  const transactions = data.transactions.map((transaction): ActivityItem => {
    const mark = transactionMarkKind(transaction)
    const isCredit = transaction.classification.credit
    const account = accountDisplayName(
      transaction.accountId,
      transaction.account,
      accountDisplayNames,
    )
    const splitCategories = isCredit ? undefined : transaction.splits?.map((part) => part.category)
    const category = isCredit
      ? 'Credit'
      : splitCategories
        ? 'Split'
        : transaction.category || 'Other'
    const subcategory = isCredit
      ? undefined
      : (splitCategories?.join(', ') ?? transaction.subcategory)
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
      subcategory,
      title: transaction.merchant,
      detail: [account, category, subcategory].filter(Boolean).join(' · '),
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
      benefit: benefitNote(transaction.id),
      tags: transaction.tags,
      splitCategories,
      other: Boolean(transaction.other),
      flag: flag(transaction.id),
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
      // Trades show the company's logo, like Holdings; without external logos they keep the trade mark.
      logoUrl: externalLogosEnabled && trade.ticker ? stockLogoUrl(trade.ticker, true) : undefined,
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
  const daysAgo = daysBetween(activityDay, referenceDay)

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

/** Running totals for a filtered list, in whole cents. Transfers and trades are neither. */
export function activityTotals(activities: ActivityItem[]) {
  let spent = 0
  let income = 0
  for (const { kind, amount } of activities) {
    const value = cents(Math.abs(amount))
    if (kind === 'spending' && amount < 0) spent += value
    else if (kind === 'income' && amount > 0) income += value
  }
  return { spent: spent / 100, income: income / 100, net: (income - spent) / 100 }
}
