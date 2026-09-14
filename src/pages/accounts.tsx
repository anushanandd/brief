import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { listen } from '@tauri-apps/api/event'
import { ChevronRight } from 'lucide-react'
import { useEffect, useMemo, useState } from 'react'

import { AccountMark } from '../components/account-mark'
import { BrandMark } from '../components/brand-mark'
import { PageError, PageLoading } from '../components/data-state'
import { Card, Metric, SectionHeading } from '../components/ui'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { graphAccountShortcut, useGraphWindowShortcuts } from '../hooks/use-graph-window-shortcuts'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { isTauri } from '../lib/api'
import { formatCurrency, formatPercent, formatSecurityName } from '../lib/format'
import { getDefaultGraphWindow } from '../lib/graph-preferences'
import { getExternalLogosEnabled, stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { getSpendingAccountId } from '../lib/spending-preferences'
import { AccountDetailContent } from './account-detail'

function AccountRow({
  account,
  detail,
  displayName,
}: {
  account: Account
  detail: string
  displayName: string
}) {
  return (
    <Link
      className="account-workspace-row"
      data-keyboard-row
      data-keyboard-open
      to="/accounts/$accountId"
      params={{ accountId: account.id }}
    >
      <AccountMark type={account.type} />
      <span>
        <strong>{displayName}</strong>
        <small>{account.institution}</small>
      </span>
      <span className="account-workspace-row-detail">{detail}</span>
      <strong>{formatCurrency(account.value)}</strong>
      <ChevronRight size={15} aria-hidden="true" />
    </Link>
  )
}

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
  const query = useFinance()
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
  const navigate = useNavigate({ from: '/accounts' })
  const spendingAccountId = getSpendingAccountId()
  const visibleData = useMemo(
    () => ({
      ...data,
      accounts: data.accounts.filter(({ id }) => id !== spendingAccountId),
      transactions: data.transactions.filter(({ accountId }) => accountId !== spendingAccountId),
    }),
    [data, spendingAccountId],
  )
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

  useEffect(() => {
    const runShortcut = (shortcut: string) => {
      if ((shortcut !== 'graph-previous' && shortcut !== 'graph-next') || accounts.length < 2) {
        return
      }
      const direction = shortcut === 'graph-previous' ? -1 : 1
      const currentIndex = Math.max(
        0,
        accounts.findIndex(({ id }) => id === selectedAccount?.id),
      )
      const next = accounts[(currentIndex + direction + accounts.length) % accounts.length]
      void navigate({ search: { account: next.id === 'all' ? undefined : next.id } })
    }
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target
      const shortcut = graphAccountShortcut(event)
      if (
        !shortcut ||
        (target instanceof HTMLElement &&
          (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)))
      ) {
        return
      }
      event.preventDefault()
      runShortcut(shortcut)
    }

    let disposed = false
    let unlisten: (() => void) | undefined
    window.addEventListener('keydown', onKeyDown)
    if (isTauri()) {
      void listen<string>('graph-shortcut', (event) => runShortcut(event.payload)).then((stop) => {
        if (disposed) stop()
        else unlisten = stop
      })
    }
    return () => {
      disposed = true
      unlisten?.()
      window.removeEventListener('keydown', onKeyDown)
    }
  }, [accounts, navigate, selectedAccount?.id])

  return (
    <div className="page accounts-workspace-page">
      <WorkspaceHeader title="Accounts" />

      <nav className="account-switcher" aria-label="Connected accounts">
        <div className="account-switcher-grid">
          {accounts.map((account) => {
            const active = account.id === selectedAccount?.id
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
                  <strong>{formatCurrency(account.value)}</strong>
                </span>
              </Link>
            )
          })}
          {!accounts.length ? (
            <p className="overview-recent-empty">No connected accounts.</p>
          ) : null}
        </div>
      </nav>

      {selectedAccount ? (
        <AccountDetailContent
          key={selectedAccount.id}
          account={selectedAccount}
          data={visibleData}
          embedded
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
            tone={gain == null ? 'muted' : gain > 0 ? 'positive' : gain < 0 ? 'negative' : 'muted'}
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
            {!investments.length ? (
              <p className="overview-recent-empty">No investment accounts.</p>
            ) : null}
          </div>
        </Card>

        <Card className="account-group-card portfolio-positions-card">
          <SectionHeading
            title="Largest positions"
            detail="Current committed values across investment accounts."
          />
          <div className="portfolio-position-list">
            {positions.slice(0, 8).map((holding) => (
              <div
                className="portfolio-position-row"
                key={`${holding.accountId}:${holding.ticker}`}
              >
                <BrandMark
                  className="asset-mark"
                  fallback={stockMarkLabel(holding.ticker)}
                  label={`${holding.name} logo`}
                  src={stockLogoUrl(holding.ticker, externalLogos)}
                  style={{ backgroundColor: stockMarkColor(holding.ticker) }}
                />
                <span>
                  <strong>{holding.ticker}</strong>
                  <small>{formatSecurityName(holding.name)}</small>
                </span>
                <span>
                  <strong>{formatCurrency(holding.value)}</strong>
                  <small className={(holding.totalChangePct ?? 0) >= 0 ? 'positive' : 'negative'}>
                    {formatPercent(holding.totalChangePct)}
                  </small>
                </span>
              </div>
            ))}
            {!positions.length ? (
              <p className="overview-recent-empty">No valued positions.</p>
            ) : null}
          </div>
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
          {!cash.length ? <p className="overview-recent-empty">No cash accounts.</p> : null}
        </div>
      </Card>
    </div>
  )
}
