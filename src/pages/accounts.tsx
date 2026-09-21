import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { AccountMark } from '../components/account-mark'
import { AccountRow } from '../components/account-row'
import { PageError, PageLoading } from '../components/data-state'
import { PositionTable } from '../components/position-table'
import { Card, EmptyState, Metric, SectionHeading } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
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
import { getExternalLogosEnabled } from '../lib/logos'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { AccountOverview } from './account-overview'

function sumKnown(values: Array<number | null | undefined>) {
  const known = values.filter((value): value is number => value != null)
  return known.length ? known.reduce((total, value) => total + value, 0) : values.length ? null : 0
}

function accountData(data: FinanceSnapshot) {
  const accounts = data.accounts.filter(({ id }) => id !== 'all')
  return {
    investments: accounts.filter(({ type }) => type === 'brokerage' || type === 'retirement'),
    cash: accounts.filter(({ type }) => type === 'cash'),
  }
}

function useAccountsPage() {
  const query = useLiveFinance()
  if (query.isLoading) return { state: 'loading' as const }
  if (query.isError || !query.data) return { state: 'error' as const }
  return {
    state: 'ready' as const,
    data: query.data,
    names: getAccountDisplayNames(),
    externalLogos: getExternalLogosEnabled(),
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
  const live = useLiveFinance()
  const startDates = getAccountStartDates()
  const now = Date.now() / 1_000
  const valuationTime = Date.parse(live.valuationAsOf ?? data.updatedAt) / 1_000
  const navigate = useNavigate({ from: '/accounts' })
  const visibleData = useMemo(() => {
    const creditIds = new Set(
      data.accounts.filter(({ type }) => type === 'credit').map(({ id }) => id),
    )
    return {
      ...data,
      accounts: data.accounts.filter(({ id }) => !creditIds.has(id)),
      transactions: data.transactions.filter(
        ({ accountId }) => !accountId || !creditIds.has(accountId),
      ),
    }
  }, [data])
  const accounts = useMemo(() => {
    const allAccount: Account = visibleData.accounts.find(({ id }) => id === 'all') ?? {
      id: 'all',
      name: 'All accounts',
      institution: 'Brief',
      type: 'combined',
      value: data.netWorth,
    }
    return [
      allAccount,
      ...visibleData.accounts
        .filter(({ id }) => id !== 'all')
        .toSorted((left, right) => {
          if (left.value == null) return right.value == null ? 0 : 1
          if (right.value == null) return -1
          return right.value - left.value
        }),
    ]
  }, [data.netWorth, visibleData.accounts])
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

      <nav className="account-switcher" aria-label="Connected accounts">
        <div className="account-switcher-grid">
          {accounts.map((account) => {
            const active = account.id === selectedAccount?.id
            const chartId =
              account.id === 'all' || account.type === 'combined' ? 'net-worth' : account.id
            const view = accountValueChart(
              visibleData,
              account,
              live.marketSeries[chartId] ?? [],
              Number.isFinite(valuationTime) ? valuationTime : now,
              graphWindow,
              now,
              accountStartDate(visibleData, chartId, startDates),
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
          account={selectedAccount}
          data={visibleData}
          graphWindow={graphWindow}
          onGraphWindowChange={setGraphWindow}
        />
      ) : null}
    </div>
  )
}

export function InvestmentAccountsPage() {
  const page = useAccountsPage()
  if (page.state === 'loading') return <PageLoading />
  if (page.state === 'error') return <PageError />

  const { data, names, externalLogos } = page
  const { investments } = accountData(data)
  const positions = data.holdings
    .filter(({ accountId }) => investments.some(({ id }) => id === accountId))
    .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity))
  const total =
    data.brokeragePerformance.find(({ accountId }) => accountId === 'total')?.currentValue ??
    sumKnown(investments.map(({ value }) => value))
  const basis = sumKnown(investments.map(({ knownCostBasis }) => knownCostBasis))
  const gain = sumKnown(investments.map(({ knownUnrealizedGain }) => knownUnrealizedGain))
  const income = sumKnown(investments.map(({ investmentIncomeYtd }) => investmentIncomeYtd))
  const leading = positions[0]
  const concentration = leading?.value != null && total ? (leading.value / total) * 100 : null

  return (
    <div className="page accounts-workspace-page">
      <WorkspaceHeader title="Investments" parent={{ label: 'Accounts', to: '/accounts' }} />

      <Card className="workspace-brief-card">
        <header className="workspace-brief-heading">
          <div>
            <span className="balance-label">Combined portfolio</span>
            <strong className="hero-number">{formatCurrency(total)}</strong>
          </div>
          <p>Provider-backed values across {investments.length} accounts</p>
        </header>
        <div className="workspace-metric-grid">
          <Metric label="Known cost basis" value={formatCurrency(basis)} />
          <Metric
            label="Known unrealized P/L"
            value={formatCurrency(gain)}
            tone={valueTone(gain)}
          />
          <Metric label="Investment income YTD" value={formatCurrency(income)} />
          <Metric
            label="Largest position"
            value={leading?.ticker ?? '—'}
            detail={concentration == null ? undefined : `${concentration.toFixed(1)}% of portfolio`}
          />
        </div>
      </Card>

      <div className="accounts-overview-grid investment-workspace-grid">
        <Card className="account-group-card">
          <SectionHeading
            title="Investment accounts"
            detail="Open an account for its complete statement."
          />
          <div className="account-workspace-list">
            {investments.map((account) => (
              <AccountRow
                key={account.id}
                account={account}
                displayName={accountDisplayName(account.id, account.name, names)}
                detail={`${data.holdings.filter(({ accountId }) => accountId === account.id).length} positions · ${formatCurrency(account.knownUnrealizedGain)} P/L`}
              />
            ))}
            {!investments.length ? <EmptyState>No investment accounts.</EmptyState> : null}
          </div>
        </Card>

        <Card className="account-group-card portfolio-positions-card">
          <PositionTable
            title="Largest positions"
            positions={positions.slice(0, 8)}
            externalLogosEnabled={externalLogos}
            view="summary"
            emptyMessage="No positions in these accounts."
          />
        </Card>
      </div>
    </div>
  )
}

export function CashAccountsPage() {
  const page = useAccountsPage()
  if (page.state === 'loading') return <PageLoading />
  if (page.state === 'error') return <PageError />

  const { data, names } = page
  const { cash } = accountData(data)
  const cashValue = sumKnown(cash.map(({ value }) => value))
  const cashTransactions = data.transactions.filter(({ accountId }) =>
    cash.some(({ id }) => id === accountId),
  )

  return (
    <div className="page accounts-workspace-page">
      <WorkspaceHeader title="Cash" parent={{ label: 'Accounts', to: '/accounts' }} />

      <Card className="workspace-brief-card">
        <header className="workspace-brief-heading">
          <div>
            <span className="balance-label">Available cash</span>
            <strong className="hero-number">{formatCurrency(cashValue)}</strong>
          </div>
          <p>Committed provider balances</p>
        </header>
        <div className="workspace-metric-grid workspace-metric-grid-three">
          <Metric label="Accounts" value={String(cash.length)} />
          <Metric label="Transactions" value={String(cashTransactions.length)} />
          <Metric
            label="Institutions"
            value={String(new Set(cash.map(({ institution }) => institution)).size)}
          />
        </div>
      </Card>

      <Card className="account-group-card cash-accounts-list">
        <SectionHeading
          title="Cash accounts"
          detail="Checking, savings, and other cash balances."
        />
        <div className="account-workspace-list">
          {cash.map((account) => (
            <AccountRow
              key={account.id}
              account={account}
              displayName={accountDisplayName(account.id, account.name, names)}
              detail={`${data.transactions.filter(({ accountId }) => accountId === account.id).length} imported transactions`}
            />
          ))}
          {!cash.length ? <EmptyState>No cash accounts.</EmptyState> : null}
        </div>
      </Card>
    </div>
  )
}
