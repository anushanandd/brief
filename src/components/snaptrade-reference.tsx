import { formatCurrency, formatInstant } from '../lib/format'
import type { Account } from '../lib/schema'

export function SnapTradeReference({ account }: { account?: Account }) {
  if (!account?.id.startsWith('snaptrade:')) return null

  const timestamp = [account.balanceAsOf, account.balanceFetchedAt].find(
    (value) => value && Number.isFinite(Date.parse(value)),
  )

  return (
    <span className="snaptrade-reference">
      SnapTrade: {formatCurrency(account.reportedBalance)}{' '}
      {timestamp ? (
        <time dateTime={timestamp}>
          ({formatInstant(timestamp, { month: 'short', day: 'numeric', year: 'numeric' })})
        </time>
      ) : (
        '(date unavailable)'
      )}
    </span>
  )
}
