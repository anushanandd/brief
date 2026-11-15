import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { snapshotKey, useFinance } from '../hooks/use-finance'
import { getActivityTotals } from '../lib/api'
import { countOf, formatCurrencyIn, valueTone } from '../lib/format'

/**
 * The ledger's total while it is narrowed: of the selection if there is one, and otherwise of
 * every row the filters keep. It reads as one line under the ledger: how many rows, then per
 * currency (never added across them) what came in and went out when there was both, and the
 * net, bold and unlabeled under the amounts.
 * Rust sums the rows from their IDs (`get_activity_totals`); this only formats what it returns.
 * While the next sum loads the last one stays, and a sum Rust couldn't give shows no figure,
 * never a zero. Rows Rust left out, for having no amount, are counted, so a partial sum never
 * reads as complete.
 */
export function ActivityTotal({
  rows,
  selected,
}: {
  rows: readonly { id: string }[]
  /** Whether `rows` are the selection. */
  selected: boolean
}) {
  // Subscribes to the privacy mask so the total masks with the rest of the page.
  useAmountsHidden()
  const { data } = useFinance()
  // One key per set of rows, however they are ordered on screen.
  const ids = useMemo(() => rows.map(({ id }) => id).toSorted(), [rows])
  const idsKey = ids.join(',')
  const query = useQuery({
    queryKey: ['activity-totals', ...snapshotKey(data), idsKey],
    queryFn: () => getActivityTotals(ids),
    placeholderData: keepPreviousData,
    retry: false,
  })
  // An error shows no figure; a placeholder is the last set's, shown until this one's arrives.
  const totals = query.isError ? undefined : query.data
  const excluded = (totals?.withoutAmount ?? 0) + (totals?.unmatched ?? 0)
  const counts = [
    selected ? `${rows.length} selected` : countOf(rows.length, 'transaction'),
    totals?.pending ? `${totals.pending} pending` : null,
    excluded ? `${excluded} without an amount` : null,
  ].filter(Boolean)
  return (
    <div
      className="activity-total"
      role="group"
      aria-label={selected ? 'Total of the selection' : 'Total'}
    >
      <span className="activity-total-count">{counts.join(' · ')}</span>
      {totals?.currencies.map(({ currency, moneyIn, moneyOut, net }) => (
        <span className="activity-total-currency" key={currency}>
          {moneyIn && moneyOut ? (
            <>
              <span className="activity-total-part">
                In{' '}
                <span className="financial-activity-amount positive">
                  {formatCurrencyIn(moneyIn, currency)}
                </span>
              </span>
              <span className="activity-total-part">
                Out{' '}
                <span className="financial-activity-amount negative">
                  {formatCurrencyIn(moneyOut, currency)}
                </span>
              </span>
            </>
          ) : null}
          <span className="activity-total-part">
            <strong className={`financial-activity-amount ${valueTone(net)}`}>
              {formatCurrencyIn(net, currency)}
            </strong>
          </span>
        </span>
      ))}
    </div>
  )
}
