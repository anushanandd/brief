import { accountDisplayName, type AccountDisplayNames } from './account-names'
import { activityBenefitNote, type ActivityBenefitNote } from './benefit-copy'
import { daysBetween } from './dates'
import { asciiMinus, displayCase, formatCalendarDate, formatCurrency } from './format'
import { stockLogoUrl, transactionLogoUrl } from './logos'
import { inferredNeedSubjects } from './needs'
import { placeLabel } from './places'
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
  /** Where it was made, as Rust resolved it; `location` stays the bank's own report. */
  place?: Transaction['place']
  /** Whether the cardholder was there, as Rust weighed the records. */
  presence?: Transaction['presence']
  /** The moment of purchase, with the offset where it happened. */
  occurredAt?: string
  /** What Apple Wallet recorded. */
  wallet?: Transaction['wallet']
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
  /** The names of the people it's with, so "with Alex" finds it. */
  people?: string[]
  /** The categories a split transaction is divided between. */
  splitCategories?: string[]
  /** Whether a transaction is someone else's money; absent on trades, which can't be. */
  other?: boolean
  /** A second look Rust suggests: a possible duplicate, an unusual amount, details whose
   * sources disagree, or an inferred need (a new series, a new price, a fee, a paycheck
   * outside its band). */
  flag?: 'duplicate' | 'unusual' | 'conflict' | 'new' | 'price' | 'fee' | 'pay'
  /** What Rust's grading and statement reconciliation say backs the transaction. */
  evidence?: ActivityEvidence[]
  /** The other leg of a move between your accounts, as Rust paired them. */
  transfer?: ActivityTransfer
}

export type ActivityTransfer = {
  /** The other leg's activity ID. */
  id: string
  /** Whether the money went to the other account or came from it. */
  direction: 'to' | 'from'
  account: string
  accountId?: string
  /** The day the other leg posted. */
  date: string
}

/**
 * How a transaction stands against its evidence: on an imported statement, inside a
 * statement's days but missing from it, with sources that disagree, categorized or named
 * by Plaid without confidence, or categorized by Brief from your earlier choices (so every
 * category it chose for you is listed, to keep or undo).
 */
export type ActivityEvidence = 'statement' | 'unlisted' | 'conflict' | 'unsure' | 'suggested'

function activityEvidence(transaction: Transaction, unlisted: ReadonlySet<string>) {
  const quality = transaction.quality
  const tags: ActivityEvidence[] = []
  if (quality?.amount.basis === 'statement' || quality?.amount.basis === 'statementDiffers')
    tags.push('statement')
  if (unlisted.has(transaction.id)) tags.push('unlisted')
  if (quality?.grade === 'conflict') tags.push('conflict')
  if (quality?.category.basis === 'providerUnsure' || quality?.merchant?.basis === 'providerUnsure')
    tags.push('unsure')
  if (transaction.categorySource === 'suggested') tags.push('suggested')
  return tags.length ? tags : undefined
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
 * Where a purchase happened, in one place: the resolved place when there is one (the issuer's
 * address, the bank's descriptor, then its location), otherwise how it was paid. A purchase
 * made remotely shows as online, never at the address it was billed from. The second line adds
 * the street when the sources give it, the channel, or the merchant's site.
 */
export function activityPlace(activity: ActivityItem): {
  kind: 'place' | 'online' | 'store' | 'brokerage' | 'transfer' | 'none'
  primary: string
  secondary: string
} {
  // Money moved between your accounts went to, or came from, the other one.
  if (activity.transfer)
    return {
      kind: 'transfer',
      primary: `${activity.transfer.direction === 'to' ? 'To' : 'From'} ${activity.transfer.account}`,
      secondary: '',
    }
  const location = activity.location
  const method = activityMethod(activity)
  if (activity.place) {
    const street =
      activity.place.address ??
      (activity.place.precision === 'exact' ? location?.address : undefined)
    return {
      kind: 'place',
      primary: placeLabel(activity.place),
      secondary: street ? displayCase(street) : method === 'Other' ? '' : method,
    }
  }
  const country =
    location?.country && !domesticCountries.has(location.country.toUpperCase())
      ? location.country
      : ''
  const place =
    [[location?.city, location?.region].filter(Boolean).join(', '), country]
      .filter(Boolean)
      .join(' · ') || location?.postalCode
  if ((place || location?.address) && activity.presence !== 'remote')
    return {
      kind: 'place',
      primary: place || location!.address!,
      secondary: (place && location?.address) || (method === 'Other' ? '' : method),
    }
  if (onlineMethods.has(method)) {
    let host = ''
    try {
      host = activity.website ? new URL(activity.website).hostname.replace(/^www\./, '') : ''
    } catch {
      host = ''
    }
    return { kind: 'online', primary: method, secondary: host }
  }
  if (storeMethods.has(method)) return { kind: 'store', primary: method, secondary: '' }
  if (method === 'Brokerage') return { kind: 'brokerage', primary: 'Brokerage', secondary: '' }
  return { kind: 'none', primary: '—', secondary: '' }
}

export function paymentChannelLabel(channel?: string) {
  const value = channel?.trim().replaceAll(/[_-]+/g, ' ')
  return value ? value.charAt(0).toUpperCase() + value.slice(1).toLowerCase() : ''
}

const walletMethods = {
  tap: 'Tap to pay',
  app: 'In app',
  web: 'On the web',
  card: 'Physical card',
  online: 'Online',
} as const
const onlineMethods = new Set<string>(['Online', walletMethods.app, walletMethods.web])
const storeMethods = new Set<string>(['In store', walletMethods.tap, walletMethods.card])

export function activityMethod(activity: ActivityItem) {
  if (activity.kind === 'trade' || activity.accountId?.startsWith('snaptrade:')) return 'Brokerage'
  // Wallet saw how the card was presented, which outranks every reading of the records.
  if (activity.wallet?.method) return walletMethods[activity.wallet.method]
  // Rust's reading of the records outranks Plaid's channel.
  if (activity.presence === 'remote') return 'Online'
  if (activity.presence === 'inPerson') return 'In store'
  return paymentChannelLabel(activity.paymentChannel)
}

/** The time of purchase as the clock read where it happened, or nothing when unknown. */
export function activityTime(activity: Pick<ActivityItem, 'occurredAt'>) {
  const time = activity.occurredAt && /T(\d{2}):(\d{2})/.exec(activity.occurredAt)
  if (!time) return ''
  const hour = Number(time[1])
  return `${hour % 12 || 12}:${time[2]} ${hour < 12 ? 'AM' : 'PM'}`
}

export function activityMatchesSearch(activity: ActivityItem, query: string) {
  const text = query.trim().toLocaleLowerCase()
  if (!text) return true
  const fields = {
    description: `${activity.title} ${activity.description ?? ''}`,
    account: activity.account ?? '',
    category: `${activity.category} ${activity.subcategory ?? ''}`,
    tag: activity.tags?.join(' ') ?? '',
    person: activity.people?.join(' ') ?? '',
    method: activityMethod(activity),
    location: `${placeLabel(activity.place)} ${activityLocationText(activity.location)}`,
    amount: `${activity.amount} ${formatCurrency(activity.amount)} ${asciiMinus(formatCurrency(activity.amount))}`,
    date: activity.date,
  }
  const [scoped, field, value = ''] =
    /^(description|account|category|tag|person|method|location|amount|date):\s*(.*)$/.exec(text) ??
    []
  if (scoped && !value.trim()) return false
  const source = scoped
    ? (Object.entries(fields).find(([name]) => name === field)?.[1] ?? '')
    : `${Object.values(fields).join(' ')} ${activity.detail}`
  const terms = (scoped ? value : text).split(/\s+/).filter(Boolean)
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

export function sortActivities(activities: ActivityItem[], sort: ActivitySort) {
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
                ? transactionDateKey(activity.date)
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
  const peopleNames = new Map((data.settings?.people ?? []).map(({ id, name }) => [id, name]))
  const duplicates = new Set(
    data.insights?.anomalies.duplicates.flatMap(({ transactionIds }) => transactionIds),
  )
  const unusual = new Set(
    data.insights?.anomalies.unusual.map(({ transactionId }) => transactionId),
  )
  const unlisted = new Set((data.statementReconciliation ?? []).flatMap(({ extra }) => extra))
  const inferred = inferredNeedSubjects(data.insights?.brief.needs ?? [])
  const inferredFlags = {
    'new-series': 'new',
    'price-changed': 'price',
    'fees-charged': 'fee',
    'pay-differs': 'pay',
    'unusual-charge': 'unusual',
  } as const
  // A conflict the user reviewed stays graded but no longer asks for a look.
  const flag = (transaction: Transaction) => {
    if (duplicates.has(transaction.id)) return 'duplicate' as const
    if (unusual.has(transaction.id)) return 'unusual' as const
    if (transaction.quality?.grade === 'conflict' && !transaction.reviewed)
      return 'conflict' as const
    const need = inferred.get(transaction.id)
    return need ? inferredFlags[need.kind] : undefined
  }
  const benefitNote = (id: string) => {
    const item = benefitActivity.get(id)
    if (!item) return undefined
    const paired = item.pairedId
      ? (benefitActivity.get(item.pairedId) ?? transactionsById.get(item.pairedId))
      : undefined
    return activityBenefitNote(item, paired) ?? undefined
  }
  const transferOf = (transaction: Transaction): ActivityTransfer | undefined => {
    const pair = transaction.transferPairId
      ? transactionsById.get(transaction.transferPairId)
      : undefined
    return pair
      ? {
          id: `spending:${pair.id}`,
          direction: transaction.amount < 0 ? 'to' : 'from',
          account: accountDisplayName(pair.accountId, pair.account, accountDisplayNames),
          accountId: pair.accountId,
          date: pair.postedOn ?? pair.date,
        }
      : undefined
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
      place: transaction.place,
      presence: transaction.presence,
      occurredAt: transaction.occurredAt,
      wallet: transaction.wallet,
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
      people: transaction.people?.flatMap((id) => peopleNames.get(id) ?? []),
      splitCategories,
      other: Boolean(transaction.other),
      flag: flag(transaction),
      evidence: activityEvidence(transaction, unlisted),
      transfer: transferOf(transaction),
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
      transactionDateKey(right.date).localeCompare(transactionDateKey(left.date)) ||
      (right.occurredAt ?? '').localeCompare(left.occurredAt ?? '') ||
      right.id.localeCompare(left.id),
  )
}

export function activityDateGroup(date: string, referenceIso: string) {
  const activityDay = transactionDateKey(date)
  const referenceDay = transactionDateKey(referenceIso)
  const daysAgo = daysBetween(activityDay, referenceDay)

  if (daysAgo < 0) return 'Upcoming'
  if (daysAgo === 0) return 'Today'
  if (daysAgo === 1) return 'Yesterday'
  if (!activityDay) return 'Unknown date'
  if (daysAgo <= 7 && activityDay.slice(0, 7) === referenceDay.slice(0, 7)) return 'Last week'
  return formatCalendarDate(activityDay, {
    month: 'long',
    ...(activityDay.slice(0, 4) === referenceDay.slice(0, 4) ? {} : { year: 'numeric' }),
  })
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
