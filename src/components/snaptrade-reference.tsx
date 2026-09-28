import { formatCurrency } from '../lib/format'
import type { Account } from '../lib/schema'

const dateFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  year: 'numeric',
})

export function SnapTradeReference({ account }: { account?: Account }) {
  if (!account?.id.startsWith('snaptrade:')) return null

  const timestamp = [account.balanceAsOf, account.balanceFetchedAt].find(
    (value) => value && Number.isFinite(Date.parse(value)),
  )

  return (
    <span className="snaptrade-reference">
      SnapTrade: {formatCurrency(account.reportedBalance)}{' '}
      {timestamp ? (
        <time dateTime={timestamp}>({dateFormatter.format(new Date(timestamp))})</time>
      ) : (
        '(date unavailable)'
      )}
    </span>
  )
}
