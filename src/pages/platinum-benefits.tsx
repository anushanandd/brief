import { Link } from '@tanstack/react-router'
import { Settings } from 'lucide-react'

import { PageError, PageLoading } from '../components/data-state'
import { PlatinumBenefits } from '../components/platinum-benefits'
import { SpendingAccountEmptyState } from '../components/spending-account-empty-state'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { resolveSpendingAccount } from '../lib/spending'
import { getSpendingAccountId } from '../lib/spending-preferences'

export function PlatinumBenefitsPage() {
  const query = useFinance()
  const data = query.data
  if (query.isLoading) return <PageLoading />
  if (query.isError || !data) return <PageError />
  const account = resolveSpendingAccount(data.accounts, getSpendingAccountId())

  return (
    <div className="page platinum-benefits-page">
      <WorkspaceHeader
        title="Platinum benefits"
        parent={{ label: 'Spending', to: '/spending' }}
        actions={
          <Link
            className="button-base button-ghost button-compact"
            to="/settings"
            hash="platinum-benefits"
          >
            <Settings size={16} aria-hidden="true" />
            <span className="sr-only">Manage benefits</span>
          </Link>
        }
      />
      {account ? (
        <PlatinumBenefits
          transactions={data.transactions.filter(({ accountId }) => accountId === account.id)}
          snapshotIso={data.updatedAt}
        />
      ) : (
        <SpendingAccountEmptyState />
      )}
    </div>
  )
}
