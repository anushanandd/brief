import { validDate } from './dates'
import type { FinanceSnapshot } from './schema'
import { transactionDateKey } from './spending'

export type AccountStartDates = Record<string, string>
type StartDateData = Pick<
  FinanceSnapshot,
  'accounts' | 'accountLinks' | 'transactions' | 'trades' | 'updatedAt'
>

export function parseAccountStartDates(value: unknown): AccountStartDates {
  if (typeof value !== 'string') return {}
  try {
    const parsed: unknown = JSON.parse(value)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {}
    return Object.fromEntries(
      Object.entries(parsed).filter(
        (entry): entry is [string, string] =>
          Boolean(entry[0]) && validDate(entry[1]) !== undefined,
      ),
    )
  } catch {
    return {}
  }
}

function inferredStartDate(data: StartDateData, accountId: string) {
  const relatedIds = new Set([
    accountId,
    ...Object.entries(data.accountLinks ?? {}).flatMap(([plaidId, snaptradeId]) =>
      snaptradeId === accountId ? [plaidId] : [],
    ),
  ])
  // One pass keeping the earliest valid date; only a new earliest date is validated.
  let earliest: string | undefined
  const consider = (value: string) => {
    const date = transactionDateKey(value, data.updatedAt)
    if ((earliest === undefined || date < earliest) && validDate(date)) earliest = date
  }
  for (const transaction of data.transactions)
    if (transaction.accountId && relatedIds.has(transaction.accountId))
      consider(transaction.postedOn ?? transaction.date)
  for (const trade of data.trades) if (relatedIds.has(trade.accountId)) consider(trade.date)
  return earliest
}

export function accountStartDate(
  data: StartDateData,
  accountId: string,
  savedDates: AccountStartDates,
) {
  const accountIds =
    accountId === 'net-worth'
      ? data.accounts.filter(({ id }) => id !== 'all').map(({ id }) => id)
      : accountId === 'total'
        ? data.accounts
            .filter(({ type }) => type === 'brokerage' || type === 'retirement')
            .map(({ id }) => id)
        : [accountId]
  return accountIds
    .flatMap((id) => savedDates[id] ?? inferredStartDate(data, id) ?? [])
    .toSorted()[0]
}
