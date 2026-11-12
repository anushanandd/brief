import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { snapshotKey, useFinance } from '../hooks/use-finance'
import { getActivityTotals } from '../lib/api'
import { formatCurrencyIn, valueTone } from '../lib/format'

/**
 * The ledger's total while it is narrowed: the net of the selection if there is one, and
 * otherwise of every row the filters keep, as one figure under the Amount column (one per
 * currency, never added across them). Rust sums the rows from their IDs
 * (`get_activity_totals`); this only formats what it returns. While the next sum loads the last
 * one stays, and a sum Rust couldn't give shows no figure, never a zero. Rows Rust left out, for
 * having no amount, are counted beside it, so a partial sum never reads as complete. It renders
 * as the cells of the ledger's footer row (`GroupedActivityList`'s `footer`).
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
  return (
    <>
      <span className="activity-total-label" aria-hidden="true">
        Total
      </span>
      <span
        className="activity-total-figures"
        role="group"
        aria-label={selected ? 'Total of the selection' : 'Total'}
      >
        {totals && excluded ? (
          <small className="activity-total-note">excludes {excluded}</small>
        ) : null}
        {totals?.currencies.map(({ currency, net }) => (
          <span className={`financial-activity-amount ${valueTone(net)}`} key={currency}>
            {formatCurrencyIn(net, currency)}
          </span>
        ))}
      </span>
    </>
  )
}
