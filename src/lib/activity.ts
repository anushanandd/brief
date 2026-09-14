import { accountDisplayName, type AccountDisplayNames } from './account-name-preferences'
import { formatCurrency } from './format'
import { transactionLogoUrl, transactionMarkKind } from './logos'
import type { AccountMovement, FinanceSnapshot, SnapshotChange } from './schema'
import { isSpendingTransaction, transactionDateKey } from './spending'

const accountMovementLimit = 2_000
export function movementsFromChange(change?: SnapshotChange): AccountMovement[] {
  return (change?.accountChanges ?? []).map((account) => ({
    id: `${change?.observedAt}:${account.accountId}`,
    observedAt: change?.observedAt ?? '',
    accountId: account.accountId,
    name: account.name,
    change: account.change,
  }))
}

export function mergeAccountMovements(
  previous: FinanceSnapshot,
  latest?: SnapshotChange,
): AccountMovement[] {
  const movements = new Map(
    [...previous.accountMovements, ...movementsFromChange(previous.lastChange)].map((movement) => [
      movement.id,
      movement,
    ]),
  )
  for (const movement of movementsFromChange(latest)) movements.set(movement.id, movement)
  return [...movements.values()]
    .toSorted((left, right) => left.observedAt.localeCompare(right.observedAt))
    .slice(-accountMovementLimit)
}

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
  logoUrl?: string
  website?: string
}

export function buildActivities(
  data: FinanceSnapshot,
  accountDisplayNames: AccountDisplayNames = {},
  externalLogosEnabled = false,
): ActivityItem[] {
  const transactions = data.transactions.map((transaction): ActivityItem => {
    const mark = transactionMarkKind(transaction)
    const category = transaction.category || 'Other'
    const isCredit =
      transaction.amount > 0 &&
      (mark === 'refund' || /\bcredit\b/i.test(`${transaction.category} ${transaction.merchant}`))
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
