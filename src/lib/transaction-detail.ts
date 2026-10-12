import type { FinanceSnapshot, Transaction } from './schema'

const byRecency = (left: Transaction, right: Transaction) =>
  (right.postedOn ?? right.date).localeCompare(left.postedOn ?? left.date) ||
  right.id.localeCompare(left.id)

const merchantKey = (merchant: string) => merchant.trim().toLocaleLowerCase()

/**
 * Everything saved about one ledger transaction, gathered for the details dialog: other activity
 * at the same merchant, its recurring schedule when one is expected, benefit evidence, and where
 * its category came from.
 */
export function transactionDetail(data: FinanceSnapshot, activityId: string, limit = 8) {
  if (!activityId.startsWith('spending:')) return undefined
  const id = activityId.slice('spending:'.length)
  const transaction = data.transactions.find((candidate) => candidate.id === id)
  if (!transaction) return undefined
  // Every transaction carries a grouping key; only an expected next charge makes it recurring.
  const expected = transaction.recurringId
    ? data.insights?.expected.find(({ recurringId }) => recurringId === transaction.recurringId)
    : undefined
  const merchant = merchantKey(transaction.merchant)
  const history = data.transactions
    .filter(
      (candidate) =>
        candidate.id !== transaction.id &&
        candidate.pendingTransactionId !== transaction.id &&
        merchantKey(candidate.merchant) === merchant,
    )
    .toSorted(byRecency)
  return {
    transaction,
    expected,
    history: history.slice(0, limit),
    historyCount: history.length,
    benefits: (data.insights?.benefits?.activity ?? []).filter((item) => item.id === id),
    categoryOrigin:
      transaction.categorySource === 'manual'
        ? ('manual' as const)
        : transaction.categorySource === 'rule'
          ? ('rule' as const)
          : ('provider' as const),
    rule: transaction.ruleId
      ? data.rules?.find((candidate) => candidate.id === transaction.ruleId)
      : undefined,
    ...transactionFlags(data, id),
  }
}

/** What Rust flagged about one transaction, looked up from its insights. */
export function transactionFlags(data: FinanceSnapshot, id: string) {
  const insights = data.insights
  const duplicate = insights?.anomalies.duplicates.find(({ transactionIds }) =>
    transactionIds.includes(id),
  )
  return {
    duplicate: duplicate
      ? { ...duplicate, otherId: duplicate.transactionIds.find((other) => other !== id)! }
      : undefined,
    unusual: insights?.anomalies.unusual.find(({ transactionId }) => transactionId === id),
    trip: insights?.trips.trips.find(({ transactionIds }) => transactionIds.includes(id)),
    priceChange: insights?.subscriptions.items.find(
      ({ priceChange }) => priceChange?.transactionId === id,
    )?.priceChange,
  }
}
/**
 * Opens the place in Apple Maps, at its coordinates when the bank reported them; nothing is
 * sent until the person follows the link.
 */
export function mapsUrl(address: string, coordinates?: { lat?: number; lon?: number }) {
  if (coordinates?.lat != null && coordinates.lon != null)
    return `https://maps.apple.com/?ll=${coordinates.lat},${coordinates.lon}&q=${encodeURIComponent(address)}`
  return `https://maps.apple.com/?q=${encodeURIComponent(address)}`
}

/** Categories already in use, for choosing one; the current category is always offered. */
export function categoryChoices(data: FinanceSnapshot, current: string) {
  return [...new Set([current, ...data.transactions.map(({ category }) => category)])]
    .filter((category) => category.trim())
    .toSorted((left, right) => left.localeCompare(right))
}
