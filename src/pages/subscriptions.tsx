import { RefreshCw } from 'lucide-react'

import { PageError, PageLoading } from '../components/data-state'
import { ActivityDetailHeader } from '../components/spending-detail-header'
import { Card, SectionHeading } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { formatCurrency } from '../lib/format'
import { formatActivityDate, identifySubscriptions, resolveSpendingAccount } from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

export function SubscriptionsPage() {
  const query = useFinance()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const account = resolveSpendingAccount(data.accounts, getSpendingAccountId())
  const accountName = account
    ? accountDisplayName(account.id, account.name, getAccountDisplayNames())
    : undefined
  const subscriptions = identifySubscriptions(
    account ? data.transactions.filter(({ accountId }) => accountId === account.id) : [],
    data.updatedAt,
  )

  return (
    <div className="page spending-detail-page">
      <ActivityDetailHeader title="Subscriptions" accountName={accountName} />
      {account ? (
        <Card className="subscription-card">
          <SectionHeading
            title="Possible subscriptions"
            detail="Detected from similar posted charges at regular monthly, quarterly, or annual intervals. Verify each charge before relying on this list."
            action={<strong className="subscription-count">{subscriptions.length}</strong>}
          />
          <div className="subscription-list">
            {subscriptions.map((subscription) => (
              <article className="subscription-row" key={subscription.merchant}>
                <span
                  className="workspace-destination-icon transaction-mark-payment"
                  aria-hidden="true"
                >
                  <RefreshCw size={16} />
                </span>
                <span>
                  <strong>{subscription.merchant}</strong>
                  <small>
                    {subscription.cadence} · {subscription.occurrences} matching charges
                  </small>
                </span>
                <span>
                  <strong>{formatCurrency(subscription.latestAmount)}</strong>
                  <small>
                    Last charged {formatActivityDate(subscription.lastChargedOn, data.updatedAt)}
                  </small>
                </span>
              </article>
            ))}
            {!subscriptions.length ? (
              <p className="overview-recent-empty">No recurring charge patterns found.</p>
            ) : null}
          </div>
        </Card>
      ) : null}
    </div>
  )
}
