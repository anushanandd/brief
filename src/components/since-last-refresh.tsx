import { accountDisplayName, type AccountDisplayNames } from '../lib/account-names'
import {
  countOf,
  formatActivityName,
  formatCurrency,
  formatShortDay,
  formatSignedCurrency,
} from '../lib/format'
import type { FinanceSnapshot, TransactionChange } from '../lib/schema'
import { Ban, Check, History, ReceiptText } from './icons'

/** The changes worth reading on their own: settled, removed, and amended charges. */
const rowsShown = 4

/**
 * What the last refresh changed, from native change records: how far balances moved, charges
 * that settled for a different amount or vanished, and how many arrived. Hidden when it changed
 * nothing.
 */
export function SinceLastRefresh({
  data,
  accountNames,
}: {
  data: FinanceSnapshot
  accountNames: AccountDisplayNames
}) {
  const change = data.lastChange
  if (!change || !hasRefreshChanges(data)) return null
  const changes = change.transactionChanges ?? []
  const notable = changes.filter(({ kind }) => kind !== 'added')
  const added = changes.length - notable.length
  const moved = change.accountChanges.filter((account) => account.change !== 0)
  const names = new Map(data.accounts.map(({ id, name }) => [id, name]))
  const accountName = (accountId: string) =>
    accountDisplayName(accountId, names.get(accountId) ?? 'Account', accountNames)
  return (
    <section className="coming-up since-last-refresh" aria-labelledby="since-last-refresh-title">
      <header className="coming-up-heading">
        <h3 id="since-last-refresh-title">Since last refresh</h3>
      </header>
      {moved.length ? (
        <p className="coming-up-quiet">
          {change.comparisonComplete === false ? 'Refreshed accounts moved ' : 'Balances moved '}
          <span className="sensitive-value">{formatSignedCurrency(change.netWorthChange)}</span>
          {` across ${countOf(moved.length, 'account')}.`}
        </p>
      ) : null}
      {notable.length ? (
        <ul className="coming-up-list">
          {notable.slice(0, rowsShown).map((item) => (
            <li key={item.id} className="coming-up-row">
              <time dateTime={item.date}>{formatShortDay(item.date)}</time>
              <span className="transaction-mark" aria-hidden="true">
                <ChangeIcon kind={item.kind} />
              </span>
              <span className="coming-up-copy">
                <strong>{formatActivityName(item.merchant)}</strong>
                <small>
                  {describe(item)}
                  {item.previousAmount === undefined ? null : (
                    <>
                      , was{' '}
                      <span className="sensitive-value">{formatCurrency(item.previousAmount)}</span>
                    </>
                  )}
                  {` · ${accountName(item.accountId)}`}
                </small>
              </span>
              <span className="sensitive-value">{formatCurrency(item.amount)}</span>
            </li>
          ))}
        </ul>
      ) : null}
      {added || change.backfilled || notable.length > rowsShown ? (
        <p className="coming-up-quiet">
          {[
            added ? countOf(added, 'new transaction') : null,
            notable.length > rowsShown ? `${notable.length - rowsShown} more changes` : null,
            change.backfilled ? `${countOf(change.backfilled, 'older transaction')} arrived` : null,
          ]
            .filter(Boolean)
            .join(' · ')}
        </p>
      ) : null}
    </section>
  )
}

/** Whether the last refresh moved a balance or changed a saved transaction. */
export function hasRefreshChanges(data: FinanceSnapshot) {
  const change = data.lastChange
  return Boolean(
    change &&
    (change.accountChanges.some((account) => account.change !== 0) ||
      change.transactionChanges?.length ||
      change.backfilled),
  )
}

/** What happened to each kind of change, in words; the amounts are shown beside it. */
const descriptions: Record<TransactionChange['kind'], (item: TransactionChange) => string> = {
  settled: () => 'Posted for a different amount',
  removed: (item) => (item.pending ? 'Pending charge dropped' : 'Removed by the bank'),
  amended: () => 'Amount changed',
  added: (item) => (item.pending ? 'New pending charge' : 'New'),
}

export const describe = (item: TransactionChange) => descriptions[item.kind](item)

const icons = { settled: Check, removed: Ban, amended: History, added: ReceiptText } as const

function ChangeIcon({ kind }: { kind: TransactionChange['kind'] }) {
  const Icon = icons[kind]
  return <Icon size={15} />
}
