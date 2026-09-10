import { Link, useParams } from '@tanstack/react-router'
import { AlertCircle } from 'lucide-react'
import { type ReactNode, useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { BrandMark } from '../components/brand-mark'
import { PerformanceChart } from '../components/charts'
import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { TransactionList } from '../components/transaction-list'
import { Card, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { useGraphWindowShortcuts } from '../hooks/use-graph-window-shortcuts'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { buildActivities } from '../lib/activity'
import { isTauri, openExternalUrl } from '../lib/api'
import { buildChartEventGroups } from '../lib/chart-events'
import { formatCurrency, formatPercent, formatSecurityName, formatUpdatedAt } from '../lib/format'
import { getDefaultGraphWindow } from '../lib/graph-preferences'
import { buildLiveChartData } from '../lib/live-chart'
import { getExternalLogosEnabled, stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import type { Account, FinanceSnapshot, Trade } from '../lib/schema'
import { buildSpendingView, sortTransactionsByRecency } from '../lib/spending'

const humanize = (value: string) =>
  value.replaceAll(/[_-]+/g, ' ').replace(/^./, (character) => character.toLocaleUpperCase())

export function AccountDetailPage() {
  const query = useFinance()
  const { accountId } = useParams({ from: '/accounts/$accountId' })
  const [graphWindow, setGraphWindow] = useState(getDefaultGraphWindow)
  useGraphWindowShortcuts(setGraphWindow)

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const account = query.data.accounts.find(({ id }) => id === accountId && id !== 'all')
  if (!account) {
    return (
      <div className="page centered-state">
        <AlertCircle size={24} aria-hidden="true" />
        <h1>Account not found.</h1>
        <p>This account may no longer be connected.</p>
        <Link className="button-base button-secondary button-default" to="/accounts">
          Back to Accounts
        </Link>
      </div>
    )
  }

  return (
    <AccountDetailContent
      key={account.id}
      account={account}
      data={query.data}
      graphWindow={graphWindow}
      onGraphWindowChange={setGraphWindow}
    />
  )
}

function AccountMetric({
  label,
  value,
  detail,
  tone,
}: {
  label: string
  value: ReactNode
  detail?: string
  tone?: 'positive' | 'negative' | 'muted'
}) {
  return (
    <div className="account-metric">
      <span>{label}</span>
      <strong className={tone}>{value}</strong>
      {detail ? <small>{detail}</small> : null}
    </div>
  )
}

function PositionTable({
  positions,
  externalLogosEnabled,
}: {
  positions: FinanceSnapshot['holdings']
  externalLogosEnabled: boolean
}) {
  return (
    <div className="account-positions-scroll">
      <table className="account-positions-table">
        <thead>
          <tr>
            <th scope="col">Security</th>
            <th scope="col">Shares</th>
            <th scope="col">Price</th>
            <th scope="col">Cost basis</th>
            <th scope="col">Value</th>
            <th scope="col">Unrealized P/L</th>
          </tr>
        </thead>
        <tbody>
          {positions.map((holding) => {
            const yahooUrl = `https://finance.yahoo.com/quote/${encodeURIComponent(holding.ticker)}/`
            const tone =
              holding.unrealizedGain == null
                ? 'muted'
                : holding.unrealizedGain > 0
                  ? 'positive'
                  : holding.unrealizedGain < 0
                    ? 'negative'
                    : 'muted'
            return (
              <tr key={`${holding.accountId}:${holding.ticker}`}>
                <th scope="row">
                  <a
                    className="position-security"
                    href={yahooUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    onClick={(event) => {
                      if (!isTauri()) return
                      event.preventDefault()
                      void openExternalUrl(yahooUrl)
                    }}
                  >
                    <BrandMark
                      className="asset-mark"
                      fallback={stockMarkLabel(holding.ticker)}
                      label={`${holding.name} logo`}
                      src={stockLogoUrl(holding.ticker, externalLogosEnabled)}
                      style={{ backgroundColor: stockMarkColor(holding.ticker) }}
                    />
                    <span>
                      <strong>{holding.ticker}</strong>
                      <small>{holding.valuationNote ?? formatSecurityName(holding.name)}</small>
                    </span>
                  </a>
                </th>
                <td>
                  {holding.shares?.toLocaleString('en-US', { maximumFractionDigits: 4 }) ?? '—'}
                </td>
                <td>{formatCurrency(holding.price)}</td>
                <td>{formatCurrency(holding.costBasis)}</td>
                <td>{formatCurrency(holding.value)}</td>
                <td className={tone}>
                  <strong>{formatCurrency(holding.unrealizedGain)}</strong>
                  <small>{formatPercent(holding.totalChangePct)}</small>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
      {!positions.length ? (
        <p className="overview-recent-empty">No positions in this account.</p>
      ) : null}
    </div>
  )
}

function SalesTable({
  sales,
  externalLogosEnabled,
}: {
  sales: Trade[]
  externalLogosEnabled: boolean
}) {
  return (
    <div className="account-positions-scroll">
      <table className="account-positions-table account-realized-trades-table">
        <thead>
          <tr>
            <th scope="col">Date</th>
            <th scope="col">Security</th>
            <th scope="col">Shares</th>
            <th scope="col">Sale price</th>
            <th scope="col">Proceeds</th>
            <th scope="col">FIFO basis</th>
            <th scope="col">Estimated P/L</th>
          </tr>
        </thead>
        <tbody>
          {sales.map((trade) => {
            const tone =
              trade.estimatedRealizedGain == null
                ? 'muted'
                : trade.estimatedRealizedGain > 0
                  ? 'positive'
                  : trade.estimatedRealizedGain < 0
                    ? 'negative'
                    : 'muted'
            return (
              <tr key={trade.id}>
                <td>{trade.date}</td>
                <th scope="row">
                  <span className="position-security realized-trade-security">
                    <BrandMark
                      className="asset-mark"
                      fallback={stockMarkLabel(trade.ticker ?? '—')}
                      label={`${trade.ticker ?? 'Security'} logo`}
                      src={stockLogoUrl(trade.ticker ?? '—', externalLogosEnabled)}
                      style={{ backgroundColor: stockMarkColor(trade.ticker ?? '—') }}
                    />
                    <span>
                      <strong>{trade.ticker ?? 'Security'}</strong>
                      <small>{trade.description ?? 'Sell execution'}</small>
                    </span>
                  </span>
                </th>
                <td>{trade.units?.toLocaleString('en-US', { maximumFractionDigits: 4 }) ?? '—'}</td>
                <td>{formatCurrency(trade.price)}</td>
                <td>{formatCurrency(Math.abs(trade.amount))}</td>
                <td>{formatCurrency(trade.realizedCostBasis)}</td>
                <td className={tone}>
                  <strong>{formatCurrency(trade.estimatedRealizedGain)}</strong>
                  <small>
                    {trade.estimatedRealizedGain == null
                      ? 'Lot match unavailable'
                      : formatPercent(trade.estimatedRealizedGainPct)}
                  </small>
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
      {!sales.length ? <p className="overview-recent-empty">No imported sales this year.</p> : null}
    </div>
  )
}

function AccountDetailContent({
  account,
  data,
  graphWindow,
  onGraphWindowChange,
}: {
  account: Account
  data: FinanceSnapshot
  graphWindow: number
  onGraphWindowChange: (seconds: number) => void
}) {
  const accountDisplayNames = getAccountDisplayNames()
  const externalLogosEnabled = getExternalLogosEnabled()
  const displayName = accountDisplayName(account.id, account.name, accountDisplayNames)
  const relatedAccountIds = useMemo(
    () =>
      new Set([
        account.id,
        ...Object.entries(data.accountLinks ?? {}).flatMap(([plaidId, snaptradeId]) =>
          snaptradeId === account.id ? [plaidId] : [],
        ),
      ]),
    [account.id, data.accountLinks],
  )
  const transactions = useMemo(
    () =>
      sortTransactionsByRecency(
        data.transactions.filter(({ accountId }) => accountId && relatedAccountIds.has(accountId)),
        data.updatedAt,
      ),
    [data.transactions, data.updatedAt, relatedAccountIds],
  )
  const trades = useMemo(
    () => data.trades.filter(({ accountId }) => accountId === account.id),
    [account.id, data.trades],
  )
  const sales = useMemo(
    () =>
      trades
        .filter(({ type }) => type.toUpperCase().includes('SELL'))
        .toSorted(
          (left, right) => right.date.localeCompare(left.date) || right.id.localeCompare(left.id),
        ),
    [trades],
  )
  const positions = useMemo(
    () =>
      data.holdings
        .filter(({ accountId }) => accountId === account.id)
        .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity)),
    [account.id, data.holdings],
  )
  const activities = useMemo(
    () =>
      buildActivities({ ...data, transactions, trades }, accountDisplayNames, externalLogosEnabled),
    [accountDisplayNames, data, externalLogosEnabled, trades, transactions],
  )
  const spending = useMemo(
    () => buildSpendingView(transactions, data.updatedAt, 1),
    [data.updatedAt, transactions],
  )
  const performance = data.brokeragePerformance.find(({ accountId }) => accountId === account.id)
  const isInvestment = account.type === 'brokerage' || account.type === 'retirement'
  const isCredit = account.type === 'credit'
  const chartNow = Date.parse(data.updatedAt) / 1_000
  const safeChartNow = Number.isFinite(chartNow) ? chartNow : Date.now() / 1_000
  const history = useMemo(
    () =>
      account.value == null
        ? []
        : performance?.points.length
          ? performance.points
          : [{ date: data.updatedAt.slice(0, 10), value: account.value }],
    [account.value, data.updatedAt, performance?.points],
  )
  const chartData =
    account.value == null ? [] : buildLiveChartData(history, [], account.value, safeChartNow)
  const netDeposits =
    performance?.points.length && performance.performanceMethod !== 'value-only'
      ? buildLiveChartData(
          performance.points.flatMap(({ date, netDeposits: deposited }) =>
            typeof deposited === 'number' ? [{ date, value: deposited }] : [],
          ),
          [],
          performance.points.findLast((point) => typeof point.netDeposits === 'number')
            ?.netDeposits ?? 0,
          safeChartNow,
        )
      : undefined
  const benchmarkHistory =
    performance?.performanceMethod !== 'value-only'
      ? (performance?.points.flatMap(({ date, sp500 }) =>
          typeof sp500 === 'number' ? [{ date, value: sp500 }] : [],
        ) ?? [])
      : []
  const benchmark = benchmarkHistory.length
    ? buildLiveChartData(benchmarkHistory, [], benchmarkHistory.at(-1)?.value ?? 0, safeChartNow)
    : undefined
  const chartEvents = isInvestment
    ? buildChartEventGroups(data, account.id, history, graphWindow)
    : []
  const gain = account.knownUnrealizedGain
  const gainTone = gain == null ? 'muted' : gain > 0 ? 'positive' : gain < 0 ? 'negative' : 'muted'
  const realizedGain = account.estimatedRealizedGainYtd
  const realizedGainTone =
    realizedGain == null
      ? 'muted'
      : realizedGain > 0
        ? 'positive'
        : realizedGain < 0
          ? 'negative'
          : 'muted'
  const missingBasis = positions.filter(
    ({ costBasis, value }) => costBasis == null || value == null,
  ).length
  const historyLabel =
    account.value == null
      ? 'USD balance unavailable'
      : performance?.historySource === 'provider-estimated'
        ? 'Provider-estimated value history'
        : performance?.historySource === 'reported'
          ? 'Observed provider history'
          : performance?.historySource === 'estimated'
            ? 'Estimated history'
            : 'Historical coverage unavailable'

  return (
    <div className={`page account-detail-page account-detail-${account.type}`}>
      <header className="page-header account-detail-header">
        <div>
          <h1 className="page-route">
            <Link to="/accounts">Accounts</Link>
            <span className="page-route-separator">/</span>
            <span aria-current="page">{displayName}</span>
          </h1>
          <p>
            {account.institution} · {humanize(account.type)}
          </p>
        </div>
        <div className="account-header-actions">
          <span className="freshness">
            <StatusDot /> Updated {formatUpdatedAt(data.updatedAt)}
          </span>
          <RefreshButton />
        </div>
      </header>

      <Card className="account-history-card account-overview-card">
        <header className="account-history-header">
          <div>
            <span className="balance-label">
              {isInvestment
                ? 'Portfolio value'
                : isCredit
                  ? 'Net-worth balance'
                  : 'Current balance'}
            </span>
            <strong className="hero-number">{formatCurrency(account.value)}</strong>
          </div>
          <span className="balance-institution">
            {isInvestment ? historyLabel : 'Current provider balance'}
            {performance?.historyStart ? ` · from ${performance.historyStart}` : ''}
          </span>
        </header>
        <div className="account-metric-grid">
          {isInvestment ? (
            <>
              <AccountMetric
                label={account.costBasisCoverage === 'partial' ? 'Known cost basis' : 'Cost basis'}
                value={formatCurrency(account.knownCostBasis)}
                detail={
                  missingBasis
                    ? `${missingBasis} positions missing basis or value`
                    : 'Provider-reported positions'
                }
              />
              <AccountMetric
                label={
                  account.costBasisCoverage === 'partial'
                    ? 'Known unrealized P/L'
                    : 'Unrealized P/L'
                }
                value={formatCurrency(gain)}
                detail={formatPercent(account.knownUnrealizedGainPct)}
                tone={gainTone}
              />
              <AccountMetric
                label="Investment income YTD"
                value={formatCurrency(account.investmentIncomeYtd)}
              />
              <AccountMetric
                label="Positions"
                value={positions.length}
                detail={`${trades.length} imported trades`}
              />
            </>
          ) : isCredit ? (
            <>
              <AccountMetric label="Spending this month" value={formatCurrency(spending.total)} />
              <AccountMetric
                label="Pending spending"
                value={formatCurrency(spending.pendingTotal)}
              />
              <AccountMetric
                label="Largest purchase"
                value={formatCurrency(spending.biggest ? Math.abs(spending.biggest.amount) : null)}
                detail={spending.biggest?.merchant}
              />
              <AccountMetric label="Imported transactions" value={transactions.length} />
            </>
          ) : (
            <>
              <AccountMetric label="Imported transactions" value={transactions.length} />
              <AccountMetric
                label="Pending"
                value={transactions.filter(({ pending }) => pending).length}
              />
              <AccountMetric label="Currency" value={account.currency ?? 'Unknown'} />
            </>
          )}
        </div>
        {!isInvestment ? (
          <p className="account-realized-note">
            {isCredit
              ? 'Card balances are liabilities in net worth. Spending uses posted account activity.'
              : 'Brief shows the committed provider balance and does not invent unavailable account history.'}
          </p>
        ) : null}
      </Card>

      {isInvestment ? (
        <>
          <Card className="account-history-card account-detail-section">
            <SectionHeading title="Performance" detail={historyLabel} />
            {account.value != null ? (
              <PerformanceChart
                data={chartData}
                netDeposits={netDeposits}
                benchmark={benchmark}
                value={account.value}
                events={chartEvents}
                referenceIso={data.updatedAt}
                selectedWindow={graphWindow}
                onWindowChange={onGraphWindowChange}
              />
            ) : (
              <p className="spending-detail-note">
                The provider has not supplied a supported USD balance.
              </p>
            )}
          </Card>

          <Card className="account-positions-card account-detail-section">
            <SectionHeading
              title="Positions"
              detail={`${positions.length} imported ${positions.length === 1 ? 'position' : 'positions'} · P/L uses provider cost basis.`}
            />
            <PositionTable positions={positions} externalLogosEnabled={externalLogosEnabled} />
          </Card>

          <Card className="account-history-card account-income-card account-detail-section">
            <SectionHeading
              title="Income & sales"
              detail="Year-to-date imported investment activity."
            />
            <div className="account-metric-grid">
              <AccountMetric
                label="Investment income"
                value={formatCurrency(account.investmentIncomeYtd)}
                detail="Dividends and interest"
              />
              <AccountMetric
                label="Sale proceeds"
                value={formatCurrency(account.saleProceedsYtd)}
                detail="Proceeds, not profit"
              />
              <AccountMetric label="Sales" value={account.salesYtd ?? 0} />
              <AccountMetric
                label="Estimated realized P/L"
                value={formatCurrency(realizedGain)}
                detail={realizedGain == null ? 'Reconciled lots unavailable' : 'Estimated FIFO'}
                tone={realizedGainTone}
              />
            </div>
            {account.salesYtd ? (
              <p className="account-realized-note">
                {realizedGain != null
                  ? `Estimated with FIFO from imported executions${account.realizedGainCoverage === 'partial' ? '; some sales could not be matched' : ''}. Your broker’s tax result may differ.`
                  : 'Realized P/L is unavailable because imported executions do not reconcile to enough purchase lots.'}
              </p>
            ) : null}
          </Card>
          <Card className="account-realized-trades-card account-detail-section">
            <SectionHeading
              title="Realized sales"
              detail="FIFO estimates require reconciled purchase lots."
            />
            <SalesTable sales={sales} externalLogosEnabled={externalLogosEnabled} />
          </Card>

          <Card className="account-transactions-card account-detail-section">
            <SectionHeading
              title="Investment activity"
              detail="Trades, dividends, interest, fees, and linked account activity."
            />
            <ActivityList
              activities={activities}
              referenceIso={data.updatedAt}
              emptyMessage="No imported investment activity."
            />
          </Card>
        </>
      ) : null}

      {isCredit ? (
        <Card className="account-history-card account-income-card account-detail-section">
          <SectionHeading title="This month" detail={`${spending.start} through ${spending.end}`} />
          <div className="account-metric-grid">
            <AccountMetric label="Spent" value={formatCurrency(spending.total)} />
            <AccountMetric label="Previous pace" value={formatCurrency(spending.previousTotal)} />
            <AccountMetric label="Daily pace" value={formatCurrency(spending.dailyAverage)} />
            <AccountMetric label="Pending" value={formatCurrency(spending.pendingTotal)} />
          </div>
        </Card>
      ) : null}

      {!isInvestment ? (
        <Card className="account-transactions-card account-detail-section">
          <SectionHeading
            title="Transactions"
            detail={`${transactions.length} imported transactions.`}
          />
          <TransactionList transactions={transactions} referenceIso={data.updatedAt} detailed />
        </Card>
      ) : null}
    </div>
  )
}
