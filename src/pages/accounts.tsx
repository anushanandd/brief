import { Link } from '@tanstack/react-router'
import {
  ChartNoAxesCombined,
  ChevronRight,
  CreditCard,
  Landmark,
  WalletCards,
  type LucideIcon,
} from 'lucide-react'

import { BrandMark } from '../components/brand-mark'
import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { Card, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { formatCurrency, formatPercent, formatSecurityName, formatUpdatedAt } from '../lib/format'
import { getExternalLogosEnabled, stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { buildSpendingView } from '../lib/spending'

const accountMarks: Record<string, [LucideIcon, string]> = {
  brokerage: [ChartNoAxesCombined, 'dividend'],
  retirement: [Landmark, 'interest'],
  cash: [WalletCards, 'cash'],
  credit: [CreditCard, 'payment'],
}

function AccountsHeader({ updatedAt, title = 'Accounts' }: { updatedAt: string; title?: string }) {
  return (
    <header className="page-header workspace-header">
      <h1 className={title === 'Accounts' ? undefined : 'page-route'}>
        {title === 'Accounts' ? (
          title
        ) : (
          <>
            <Link to="/accounts">Accounts</Link>
            <span className="page-route-separator">/</span>
            <span aria-current="page">{title}</span>
          </>
        )}
      </h1>
      <div className="dashboard-actions">
        <span className="freshness">
          <StatusDot /> Updated {formatUpdatedAt(updatedAt)}
        </span>
        <RefreshButton />
      </div>
    </header>
  )
}

function AccountMark({ type }: { type: string }) {
  const [Icon, tone] = accountMarks[type] ?? [Landmark, 'transfer']
  return (
    <span className={`transaction-mark transaction-mark-${tone}`} aria-hidden="true">
      <Icon size={15} />
    </span>
  )
}

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

function WorkspaceMetric({
  label,
  value,
  detail,
  tone,
}: {
  label: string
  value: string
  detail?: string
  tone?: string
}) {
  return (
    <div className="workspace-metric">
      <span>{label}</span>
      <strong className={tone}>{value}</strong>
      {detail ? <small>{detail}</small> : null}
    </div>
  )
}

function sumKnown(values: Array<number | null | undefined>) {
  const known = values.filter((value): value is number => value != null)
  return known.length ? known.reduce((total, value) => total + value, 0) : values.length ? null : 0
}

function accountData(data: FinanceSnapshot) {
  const accounts = data.accounts.filter(({ id }) => id !== 'all')
  const investments = accounts.filter(({ type }) => type === 'brokerage' || type === 'retirement')
  const cash = accounts.filter(({ type }) => type === 'cash')
  const cards = accounts.filter(({ type }) => type === 'credit')
  return { accounts, investments, cash, cards }
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

  const { data, names } = page
  const { accounts, investments, cash, cards } = accountData(data)
  const assets = sumKnown(accounts.map(({ value }) => (value == null ? value : Math.max(0, value))))
  const liabilities = sumKnown(
    cards.map(({ value }) => (value == null ? value : Math.abs(Math.min(0, value)))),
  )
  const invested = sumKnown(investments.map(({ value }) => value))
  const cashValue = sumKnown(cash.map(({ value }) => value))
  const netWorth = data.netWorth ?? (assets ?? 0) - (liabilities ?? 0)

  return (
    <div className="page accounts-workspace-page">
      <AccountsHeader updatedAt={data.updatedAt} />

      <Card className="workspace-brief-card">
        <header className="workspace-brief-heading">
          <div>
            <span className="balance-label">Total net worth</span>
            <strong className="hero-number">{formatCurrency(netWorth)}</strong>
          </div>
          <p>
            {accounts.length} connected {accounts.length === 1 ? 'account' : 'accounts'}
            {data.netWorthIncomplete ? ' · known USD balances only' : ''}
          </p>
        </header>
        <div className="workspace-metric-grid">
          <WorkspaceMetric label="Assets" value={formatCurrency(assets)} />
          <WorkspaceMetric label="Investments" value={formatCurrency(invested)} />
          <WorkspaceMetric label="Cash" value={formatCurrency(cashValue)} />
          <WorkspaceMetric
            label="Card balances"
            value={formatCurrency(liabilities)}
            detail="Counted as liabilities"
          />
        </div>
      </Card>

      <nav
        className="workspace-destination-grid accounts-destination-grid"
        aria-label="Account details"
      >
        <Link to="/accounts/investments" className="workspace-destination-card">
          <span className="workspace-destination-icon transaction-mark-dividend" aria-hidden="true">
            <ChartNoAxesCombined size={17} />
          </span>
          <span>
            <strong>Investments</strong>
            <small>
              {formatCurrency(invested)} · {investments.length}{' '}
              {investments.length === 1 ? 'account' : 'accounts'}
            </small>
          </span>
          <ChevronRight size={16} aria-hidden="true" />
        </Link>
        <Link to="/accounts/cash" className="workspace-destination-card">
          <span className="workspace-destination-icon transaction-mark-cash" aria-hidden="true">
            <WalletCards size={17} />
          </span>
          <span>
            <strong>Cash & cards</strong>
            <small>
              {cash.length} cash · {cards.length} credit
            </small>
          </span>
          <ChevronRight size={16} aria-hidden="true" />
        </Link>
      </nav>

      <Card className="account-group-card accounts-all-card">
        <SectionHeading title="All accounts" />
        <div className="account-workspace-list">
          {accounts.map((account) => (
            <AccountRow
              key={account.id}
              account={account}
              displayName={accountDisplayName(account.id, account.name, names)}
              detail={
                account.type === 'brokerage' || account.type === 'retirement'
                  ? `${data.holdings.filter(({ accountId }) => accountId === account.id).length} positions`
                  : account.type === 'credit'
                    ? 'Credit card'
                    : (account.currency ?? 'Cash')
              }
            />
          ))}
          {!accounts.length ? (
            <p className="overview-recent-empty">No connected accounts.</p>
          ) : null}
        </div>
      </Card>
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
      <AccountsHeader updatedAt={data.updatedAt} title="Investments" />

      <Card className="workspace-brief-card">
        <header className="workspace-brief-heading">
          <div>
            <span className="balance-label">Combined portfolio</span>
            <strong className="hero-number">{formatCurrency(total)}</strong>
          </div>
          <p>Provider-backed values across {investments.length} accounts</p>
        </header>
        <div className="workspace-metric-grid">
          <WorkspaceMetric label="Known cost basis" value={formatCurrency(basis)} />
          <WorkspaceMetric
            label="Known unrealized P/L"
            value={formatCurrency(gain)}
            tone={gain == null ? 'muted' : gain > 0 ? 'positive' : gain < 0 ? 'negative' : 'muted'}
          />
          <WorkspaceMetric label="Investment income YTD" value={formatCurrency(income)} />
          <WorkspaceMetric
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
  const { cash, cards } = accountData(data)
  const cashValue = sumKnown(cash.map(({ value }) => value))
  const cardBalance = sumKnown(
    cards.map(({ value }) => (value == null ? value : Math.abs(Math.min(0, value)))),
  )
  const pendingTotal = data.transactions
    .filter(({ pending, accountId }) => pending && cards.some(({ id }) => id === accountId))
    .reduce((total, transaction) => total + Math.abs(Math.min(0, transaction.amount)), 0)

  return (
    <div className="page accounts-workspace-page">
      <AccountsHeader updatedAt={data.updatedAt} title="Cash & cards" />

      <Card className="workspace-brief-card">
        <header className="workspace-brief-heading">
          <div>
            <span className="balance-label">Available cash</span>
            <strong className="hero-number">{formatCurrency(cashValue)}</strong>
          </div>
          <p>Committed provider balances · credit availability is never counted as cash</p>
        </header>
        <div className="workspace-metric-grid workspace-metric-grid-three">
          <WorkspaceMetric label="Cash accounts" value={String(cash.length)} />
          <WorkspaceMetric label="Card balances" value={formatCurrency(cardBalance)} />
          <WorkspaceMetric label="Pending card spending" value={formatCurrency(pendingTotal)} />
        </div>
      </Card>

      <div className="accounts-overview-grid">
        <Card className="account-group-card">
          <SectionHeading title="Cash" detail="Checking, savings, and other cash balances." />
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

        <Card className="account-group-card">
          <SectionHeading title="Cards" detail="Balances are liabilities in net worth." />
          <div className="account-workspace-list">
            {cards.map((account) => {
              const transactions = data.transactions.filter(
                ({ accountId }) => accountId === account.id,
              )
              const spending = buildSpendingView(transactions, data.updatedAt, 1)
              return (
                <AccountRow
                  key={account.id}
                  account={account}
                  displayName={accountDisplayName(account.id, account.name, names)}
                  detail={`${formatCurrency(spending.total)} spent this month`}
                />
              )
            })}
            {!cards.length ? <p className="overview-recent-empty">No credit accounts.</p> : null}
          </div>
        </Card>
      </div>
    </div>
  )
}
