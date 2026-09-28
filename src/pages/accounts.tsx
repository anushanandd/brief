import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { AccountMark } from '../components/account-mark'
import { PageError, PageLoading } from '../components/data-state'
import { EmptyState } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useActiveSelectionScroll } from '../hooks/use-active-selection-scroll'
import {
  useGraphAccountShortcuts,
  useGraphWindowShortcuts,
} from '../hooks/use-graph-window-shortcuts'
import { useLiveFinance } from '../hooks/use-live-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { accountStartDate, getAccountStartDates } from '../lib/account-start-date-preferences'
import { accountValueChart } from '../lib/dashboard-account-views'
import { formatCurrency, formatPercent, valueTone } from '../lib/format'
import { getDefaultGraphWindow } from '../lib/graph-preferences'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { getSpendingAccountId } from '../lib/spending-preferences'
import { AccountOverview } from './account-overview'

function useAccountsPage() {
  const query = useLiveFinance()
  if (query.isLoading) return { state: 'loading' as const }
  if (query.isError || !query.data) return { state: 'error' as const }
  return {
    state: 'ready' as const,
    data: query.data,
    names: getAccountDisplayNames(),
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
  names: ReturnType<typeof getAccountDisplayNames>
}) {
  const routeSearch = useSearch({ from: '/accounts' })
  const switcherRef = useActiveSelectionScroll()
  const live = useLiveFinance()
  const startDates = getAccountStartDates()
  const now = Date.now() / 1_000
  const valuationTime = Date.parse(live.valuationAsOf ?? data.updatedAt) / 1_000
  const navigate = useNavigate({ from: '/accounts' })
  const spendingAccountId = getSpendingAccountId()
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
  const [graphWindow, setGraphWindow] = useState(getDefaultGraphWindow)
  useGraphWindowShortcuts(setGraphWindow)
  const selectedAccount =
    accounts.find(({ id }) => id === (routeSearch.account ?? 'all')) ?? accounts[0]

  useGraphAccountShortcuts((direction) => {
    if (accounts.length < 2) return
    const currentIndex = Math.max(
      0,
      accounts.findIndex(({ id }) => id === selectedAccount?.id),
    )
    const next = accounts[(currentIndex + direction + accounts.length) % accounts.length]
    void navigate({ search: { account: next.id === 'all' ? undefined : next.id } })
  })

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

      <nav
        ref={switcherRef}
        className="account-switcher account-switcher-single-row"
        aria-label="Connected accounts"
        data-keyboard-region
      >
        <div className="account-switcher-grid account-switcher-grid-single-row">
          {accounts.map((account) => {
            const active = account.id === selectedAccount?.id
            const chartId =
              account.id === 'all' || account.type === 'combined' ? 'net-worth' : account.id
            const view = accountValueChart(
              data,
              account,
              live.marketSeries[chartId] ?? [],
              Number.isFinite(valuationTime) ? valuationTime : now,
              graphWindow,
              now,
              accountStartDate(data, chartId, startDates),
            )
            const percent =
              view.historyIsAvailable && account.type !== 'credit' ? view.chartChange.percent : null
            return (
              <Link
                className="account-switcher-button"
                activeOptions={{ exact: true }}
                data-keyboard-row
                data-keyboard-open
                aria-current={active ? 'page' : undefined}
                key={account.id}
                to="/accounts"
                search={{ account: account.id === 'all' ? undefined : account.id }}
              >
                <AccountMark type={account.type} />
                <span className="account-switcher-copy">
                  <span>{accountDisplayName(account.id, account.name, names)}</span>
                  <strong className="account-switcher-value">
                    <span>{formatCurrency(account.value)}</span>
                    <span
                      className={valueTone(percent)}
                      aria-label={
                        percent == null
                          ? 'Selected range change unavailable'
                          : `${formatPercent(percent)} balance change in selected range`
                      }
                    >
                      {formatPercent(percent)}
                    </span>
                  </strong>
                </span>
              </Link>
            )
          })}
          {!accounts.length ? <EmptyState>No connected accounts.</EmptyState> : null}
        </div>
      </nav>

      {selectedAccount ? (
        <AccountOverview
          key={selectedAccount.id}
          account={selectedAccount}
          data={data}
          graphWindow={graphWindow}
          onGraphWindowChange={setGraphWindow}
        />
      ) : null}
    </div>
  )
}
