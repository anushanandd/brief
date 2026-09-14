import type { FinanceSnapshot } from './schema'
import { transactionDateKey } from './spending'

const accountStartDatesKey = 'brief.accountStartDates'
const isoDate = /^\d{4}-\d{2}-\d{2}$/

export type AccountStartDates = Record<string, string>
type StartDateData = Pick<
  FinanceSnapshot,
  'accounts' | 'accountLinks' | 'transactions' | 'trades' | 'updatedAt'
>

function validDate(value: string) {
  if (!isoDate.test(value)) return false
  const parsed = new Date(`${value}T00:00:00Z`)
  return !Number.isNaN(parsed.getTime()) && parsed.toISOString().slice(0, 10) === value
}

export function parseAccountStartDates(value: unknown): AccountStartDates {
  if (typeof value !== 'string') return {}
  try {
    const parsed: unknown = JSON.parse(value)
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) return {}
    return Object.fromEntries(
      Object.entries(parsed).filter(
        (entry): entry is [string, string] =>
          Boolean(entry[0]) && typeof entry[1] === 'string' && validDate(entry[1]),
      ),
    )
  } catch {
    return {}
  }
}

export function getAccountStartDates() {
  return typeof window === 'undefined'
    ? {}
    : parseAccountStartDates(window.localStorage.getItem(accountStartDatesKey))
}

export function saveAccountStartDates(dates: AccountStartDates) {
  const normalized = parseAccountStartDates(JSON.stringify(dates))
  if (Object.keys(normalized).length) {
    window.localStorage.setItem(accountStartDatesKey, JSON.stringify(normalized))
  } else {
    window.localStorage.removeItem(accountStartDatesKey)
  }
  return normalized
}

function inferredStartDate(data: StartDateData, accountId: string) {
  const relatedIds = new Set([
    accountId,
    ...Object.entries(data.accountLinks ?? {}).flatMap(([plaidId, snaptradeId]) =>
      snaptradeId === accountId ? [plaidId] : [],
    ),
  ])
  return [
    ...data.transactions.flatMap((transaction) =>
      transaction.accountId && relatedIds.has(transaction.accountId)
        ? [transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)]
        : [],
    ),
    ...data.trades.flatMap((trade) =>
      relatedIds.has(trade.accountId) ? [transactionDateKey(trade.date, data.updatedAt)] : [],
    ),
  ]
    .filter(validDate)
    .toSorted()[0]
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
