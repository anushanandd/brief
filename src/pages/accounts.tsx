import { useNavigate, useRouter, useSearch } from '@tanstack/react-router'
import { useEffect, useMemo } from 'react'

import { PageError, PageLoading } from '../components/data-state'
import { WorkspaceHeader } from '../components/workspace-header'
import {
  useGraphAccountShortcuts,
  useGraphWindowShortcuts,
} from '../hooks/use-graph-window-shortcuts'
import { useLiveFinance } from '../hooks/use-live-finance'
import { accountDisplayName } from '../lib/account-names'
import { withoutSpendingCard } from '../lib/dashboard-account-views'
import { financeSettings } from '../lib/finance-settings'
import { useGraphWindow } from '../lib/graph-preferences'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { AccountOverview } from './account-overview'

function useAccountsPage() {
  const query = useLiveFinance()
  if (query.isLoading) return { state: 'loading' as const }
  if (query.isError || !query.data) return { state: 'error' as const }
  return {
    state: 'ready' as const,
    data: withoutSpendingCard(query.data),
    names: financeSettings(query.data).accountNames,
  }
}

export function AccountsPage() {
  const page = useAccountsPage()
  if (page.state === 'loading') return <PageLoading />
  if (page.state === 'error') return <PageError />

  return <AccountsWorkspace data={page.data} names={page.names} />
}

function AccountsWorkspace({
  data,
  names,
}: {
  data: FinanceSnapshot
  names: Record<string, string>
}) {
  const routeSearch = useSearch({ from: '/accounts' })
  const spendingAccountId = financeSettings(data).spendingAccountId ?? ''
  const accounts = useMemo(() => {
    const allAccount: Account = data.accounts.find(({ id }) => id === 'all') ?? {
      id: 'all',
      name: 'All accounts',
      institution: 'Brief',
      type: 'combined',
      value: data.netWorth,
    }
    return [
      allAccount,
      ...data.accounts
        .filter(({ id }) => id !== 'all' && id !== spendingAccountId)
        .toSorted((left, right) => {
          if (left.value == null) return right.value == null ? 0 : 1
          if (right.value == null) return -1
          return right.value - left.value
        }),
    ]
  }, [data.accounts, data.netWorth, spendingAccountId])
  const [graphWindow, setGraphWindow] = useGraphWindow()
  useGraphWindowShortcuts(setGraphWindow)
  const selectedAccount =
    accounts.find(({ id }) => id === (routeSearch.account ?? 'all')) ?? accounts[0]
  // The grouped list is the way in; ⌘← and ⌘→ still step through every account in its order.
  const navigate = useNavigate()
  const router = useRouter()
  const index = accounts.findIndex(({ id }) => id === selectedAccount?.id)
  const linkAt = (offset: number) => {
    const account = accounts[(index + offset + accounts.length) % accounts.length]
    return {
      to: '/accounts' as const,
      search: { account: account.id === 'all' ? undefined : account.id },
    }
  }
  useGraphAccountShortcuts((direction) => {
    if (accounts.length > 1) void navigate(linkAt(direction))
  })
  const neighbourKey =
    accounts.length > 1 ? `${linkAt(-1).search.account}:${linkAt(1).search.account}` : ''
  useEffect(() => {
    if (!neighbourKey) return undefined
    const [previous, next] = neighbourKey.split(':')
    const idle = globalThis.requestIdleCallback ?? ((run: () => void) => setTimeout(run, 50))
    const cancel = globalThis.cancelIdleCallback ?? clearTimeout
    const handle = idle(() => {
      for (const account of [previous, next])
        void router
          .preloadRoute({ to: '/accounts', search: { account: account || undefined } })
          .catch(() => undefined)
    })
    return () => cancel(handle)
  }, [neighbourKey, router])

  return (
    <div className="page accounts-workspace-page">
      <WorkspaceHeader
        title="Accounts"
        breadcrumbs={
          selectedAccount && selectedAccount.id !== 'all'
            ? [
                { label: 'Accounts', to: '/accounts', search: {} },
                {
                  label: accountDisplayName(selectedAccount.id, selectedAccount.name, names),
                  to: '/accounts',
                  search: { account: selectedAccount.id },
                },
              ]
            : undefined
        }
      />

      {selectedAccount ? (
        <AccountOverview
          key={selectedAccount.id}
          account={selectedAccount}
          data={data}
          graphWindow={graphWindow}
          onGraphWindowChange={setGraphWindow}
          excludedActivityAccountId={spendingAccountId}
        />
      ) : null}
    </div>
  )
}
