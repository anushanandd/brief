import { Link, useParams } from '@tanstack/react-router'
import { AlertCircle, ChevronRight } from 'lucide-react'
import { useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { BrandMark } from '../components/brand-mark'
import { MonthlyBarChart, PerformanceChart } from '../components/charts'
import { PageError, PageLoading, RefreshButton } from '../components/data-state'
import { PositionTable } from '../components/position-table'
import { Card, Change, Metric, SectionHeading, StatusDot } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { useGraphWindowShortcuts } from '../hooks/use-graph-window-shortcuts'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { accountStartDate, getAccountStartDates } from '../lib/account-start-date-preferences'
import { buildActivities } from '../lib/activity'
import { buildChartEventGroups } from '../lib/chart-events'
import { chartValueChange } from '../lib/dashboard-account-views'
import { formatCurrency, formatPercent, formatSecurityName, formatUpdatedAt } from '../lib/format'
import { getDefaultGraphWindow } from '../lib/graph-preferences'
import { buildLiveChartData } from '../lib/live-chart'
import {
  getExternalLogosEnabled,
  stockLogoUrl,
  stockMarkColor,
  stockMarkLabel,
  transactionMarkKind,
} from '../lib/logos'
import type { Account, FinanceSnapshot, Trade, Transaction } from '../lib/schema'
import {
  buildMonthlySpendingHistory,
  buildSpendingView,
  formatActivityDate,
  sortTransactionsByRecency,
  transactionDateKey,
} from '../lib/spending'

const humanize = (value: string) =>
  value.replaceAll(/[_-]+/g, ' ').replace(/^./, (character) => character.toLocaleUpperCase())

export function accountActivityPreviewLimit(holdingCount: number, saleCount: number) {
  const holdingRowsHeight = holdingCount ? Math.min(holdingCount, 3) * 60 : 124
  const saleRowsHeight = saleCount ? saleCount * 64 : 124
  return Math.max(1, Math.floor((109 + holdingRowsHeight + saleRowsHeight) / 60))
}

export function accountIncomeBreakdown(transactions: Transaction[], referenceIso: string) {
  const year = referenceIso.slice(0, 4)
  return transactions.reduce(
    (totals, transaction) => {
      if (
        transaction.pending ||
        transaction.amount <= 0 ||
        !transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso).startsWith(year)
      ) {
        return totals
      }
      const kind = transactionMarkKind(transaction)
      if (kind === 'dividend') totals.dividends += transaction.amount
      if (kind === 'interest') totals.interest += transaction.amount
      return totals
    },
    { dividends: 0, interest: 0 },
  )
}

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
              <tr key={trade.id} data-keyboard-row tabIndex={-1}>
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

export function AccountDetailContent({
  account,
  data,
  graphWindow,
  onGraphWindowChange,
  embedded = false,
}: {
  account: Account
  data: FinanceSnapshot
  graphWindow: number
  onGraphWindowChange: (seconds: number) => void
  embedded?: boolean
}) {
  const accountDisplayNames = getAccountDisplayNames()
  const accountStartDates = getAccountStartDates()
  const isCombined = account.id === 'all' || account.type === 'combined'
  const startDate = accountStartDate(data, isCombined ? 'net-worth' : account.id, accountStartDates)
  const externalLogosEnabled = getExternalLogosEnabled()
  const displayName = accountDisplayName(account.id, account.name, accountDisplayNames)
  const relatedAccountIds = useMemo(
    () =>
      new Set(
        isCombined
          ? data.accounts.filter(({ id }) => id !== 'all').map(({ id }) => id)
          : [
              account.id,
              ...Object.entries(data.accountLinks ?? {}).flatMap(([plaidId, snaptradeId]) =>
                snaptradeId === account.id ? [plaidId] : [],
              ),
            ],
      ),
    [account.id, data.accountLinks, data.accounts, isCombined],
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
    () => data.trades.filter(({ accountId }) => isCombined || accountId === account.id),
    [account.id, data.trades, isCombined],
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
        .filter(({ accountId }) => isCombined || accountId === account.id)
        .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity)),
    [account.id, data.holdings, isCombined],
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
  const monthlySpending = useMemo(
    () => buildMonthlySpendingHistory(transactions, data.updatedAt),
    [data.updatedAt, transactions],
  )
  const income = useMemo(
    () => accountIncomeBreakdown(transactions, data.updatedAt),
    [data.updatedAt, transactions],
  )
  const isInvestment = isCombined || account.type === 'brokerage' || account.type === 'retirement'
  const isCredit = account.type === 'credit'
  const performance = isCombined
    ? undefined
    : (isInvestment ? data.brokeragePerformance : data.accountBalanceHistory).find(
        ({ accountId }) => accountId === account.id,
      )
  const chartNow = Date.parse(data.updatedAt) / 1_000
  const safeChartNow = Number.isFinite(chartNow) ? chartNow : Date.now() / 1_000
  const history = useMemo(
    () =>
      account.value == null
        ? []
        : isCombined
          ? data.netWorthHistory
          : performance?.points.length
            ? performance.points
            : [{ date: data.updatedAt.slice(0, 10), value: account.value }],
    [account.value, data.netWorthHistory, data.updatedAt, isCombined, performance?.points],
  )
  const chartData =
    account.value == null ? [] : buildLiveChartData(history, [], account.value, safeChartNow)
  const chartChange = chartValueChange(history, account.value ?? 0, data.updatedAt.slice(0, 10))
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
    ? buildChartEventGroups(data, isCombined ? 'net-worth' : account.id, graphWindow)
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
      : isCombined
        ? data.netWorthHistoryEstimated
          ? 'Estimated history'
          : 'Observed history'
        : performance?.historySource === 'provider-estimated'
          ? 'Provider-estimated value history'
          : performance?.historySource === 'transaction-derived'
            ? 'Transaction-derived balance history'
            : performance?.historySource === 'reported'
              ? 'Observed provider history'
              : performance?.historySource === 'estimated'
                ? 'Estimated history'
                : 'Historical coverage unavailable'

  if (embedded) {
    const accountDetailLink = isCombined ? (
      'All accounts'
    ) : (
      <Link
        className="section-heading-link"
        to="/accounts/$accountId"
        params={{ accountId: account.id }}
      >
        Account details <ChevronRight size={14} aria-hidden="true" />
      </Link>
    )
    const holdingsLink = (
      <Link
        className="section-heading-link"
        to="/holdings"
        search={isCombined ? undefined : { account: account.id }}
      >
        Holdings <ChevronRight size={14} aria-hidden="true" />
      </Link>
    )
    const activityLink = (
      <Link
        className="section-heading-link"
        to="/activities"
        search={isCombined ? undefined : { account: account.id }}
      >
        Latest activity <ChevronRight size={14} aria-hidden="true" />
      </Link>
    )

    return (
      <div className={`account-overview-dashboard account-detail-${account.type}`}>
        <div className="account-overview-primary-grid">
          <Card className="account-overview-chart-card net-worth-card brokerage-performance-card">
            <header className="home-balance-header">
              <div>
                <h2 className="balance-label">{displayName}</h2>
                <div className="home-balance-value">
                  <span className="hero-number">{formatCurrency(account.value)}</span>
                </div>
                {!isCredit && account.value != null ? (
                  <div className="hero-change">
                    <span
                      className={
                        chartChange.change > 0
                          ? 'positive'
                          : chartChange.change < 0
                            ? 'negative'
                            : 'muted'
                      }
                    >
                      {formatCurrency(chartChange.change)} {chartChange.period}
                    </span>
                    <Change value={chartChange.percent} />
                  </div>
                ) : null}
              </div>
              <span className="balance-institution">
                Started {startDate ? formatActivityDate(startDate, data.updatedAt) : 'unknown'}
              </span>
            </header>

            <div className="brokerage-chart-viewport">
              <div className="brokerage-chart-slide">
                <div className="brokerage-chart-content">
                  {isCredit ? (
                    <MonthlyBarChart
                      data={monthlySpending}
                      label={`${displayName} monthly spending`}
                    />
                  ) : account.value != null ? (
                    <PerformanceChart
                      data={chartData}
                      netDeposits={netDeposits}
                      benchmark={benchmark}
                      value={account.value}
                      events={chartEvents}
                      referenceIso={data.updatedAt}
                      startDate={startDate}
                      selectedWindow={graphWindow}
                      onWindowChange={onGraphWindowChange}
                    />
                  ) : (
                    <div className="account-chart-empty account-chart-empty-compact">
                      <strong>Historical series unavailable</strong>
                      <span>Only the current provider balance is available for this account.</span>
                    </div>
                  )}
                </div>
              </div>
            </div>
          </Card>

          <Card className="account-overview-summary-card">
            <SectionHeading title={accountDetailLink} />
            <div className="account-summary-metrics">
              {isCombined ? (
                <>
                  <Metric
                    label="Accounts"
                    value={data.accounts.filter(({ id }) => id !== 'all').length}
                  />
                  <Metric label="Holdings" value={positions.length} />
                  <Metric label="Transactions" value={transactions.length} />
                  <Metric
                    label="Pending"
                    value={transactions.filter(({ pending }) => pending).length}
                  />
                  <Metric label="Trades" value={trades.length} />
                  <Metric label="Realized sales" value={sales.length} />
                </>
              ) : isInvestment ? (
                <>
                  <Metric label="Cost basis" value={formatCurrency(account.knownCostBasis)} />
                  <Metric
                    label="Unrealized P/L"
                    value={formatCurrency(gain)}
                    detail={formatPercent(account.knownUnrealizedGainPct)}
                    tone={gainTone}
                  />
                  <Metric label="Dividends" value={formatCurrency(income.dividends)} />
                  <Metric label="Interest" value={formatCurrency(income.interest)} />
                  <Metric
                    label="Sale proceeds"
                    value={formatCurrency(account.saleProceedsYtd)}
                    detail={`${sales.length} imported ${sales.length === 1 ? 'sale' : 'sales'}`}
                  />
                  <Metric
                    label="Realized P/L"
                    value={formatCurrency(realizedGain)}
                    detail={realizedGain == null ? 'Lots unavailable' : 'Estimated FIFO'}
                    tone={realizedGainTone}
                  />
                </>
              ) : isCredit ? (
                <>
                  <Metric label="Spent this month" value={formatCurrency(spending.total)} />
                  <Metric label="Pending" value={formatCurrency(spending.pendingTotal)} />
                  <Metric
                    label="Largest expense"
                    value={formatCurrency(
                      spending.biggest ? Math.abs(spending.biggest.amount) : null,
                    )}
                    detail={spending.biggest?.merchant}
                  />
                  <Metric label="Transactions" value={transactions.length} />
                </>
              ) : (
                <>
                  <Metric label="Transactions" value={transactions.length} />
                  <Metric
                    label="Pending"
                    value={transactions.filter(({ pending }) => pending).length}
                  />
                  <Metric label="Interest YTD" value={formatCurrency(income.interest)} />
                </>
              )}
            </div>
          </Card>
        </div>

        <div
          className={`account-preview-grid${isInvestment ? '' : ' account-preview-grid-simple'}`}
        >
          {isInvestment ? (
            <div className="account-investment-preview-column">
              <Card className="account-preview-card account-holdings-preview">
                <div className="account-preview-holding-head">
                  <h2>{holdingsLink}</h2>
                  <span>Shares</span>
                  <span>Price</span>
                  <span>Cost basis</span>
                  <span>Value</span>
                  <span>P/L</span>
                </div>
                <div className="account-preview-holding-list">
                  {positions.slice(0, 3).map((holding) => {
                    const tone =
                      holding.unrealizedGain == null
                        ? 'muted'
                        : holding.unrealizedGain >= 0
                          ? 'positive'
                          : 'negative'
                    return (
                      <div
                        className="account-preview-holding-row"
                        key={`${holding.accountId}:${holding.ticker}`}
                      >
                        <span className="table-asset">
                          <BrandMark
                            className="asset-mark"
                            fallback={stockMarkLabel(holding.ticker)}
                            label={`${holding.name} logo`}
                            src={stockLogoUrl(holding.ticker, externalLogosEnabled)}
                            style={{ backgroundColor: stockMarkColor(holding.ticker) }}
                          />
                          <span>
                            <strong>{holding.ticker}</strong>
                            <small>{formatSecurityName(holding.name)}</small>
                          </span>
                        </span>
                        <span>
                          {holding.shares?.toLocaleString('en-US', {
                            maximumFractionDigits: 4,
                          }) ?? '—'}
                        </span>
                        <strong>{formatCurrency(holding.price)}</strong>
                        <strong>{formatCurrency(holding.costBasis)}</strong>
                        <strong>{formatCurrency(holding.value)}</strong>
                        <span className={tone}>
                          <strong>{formatCurrency(holding.unrealizedGain)}</strong>
                          <small>{formatPercent(holding.totalChangePct)}</small>
                        </span>
                      </div>
                    )
                  })}
                  {!positions.length ? (
                    <p className="overview-recent-empty">No positions in this account.</p>
                  ) : null}
                </div>
              </Card>

              <Card className="account-preview-card account-sales-preview">
                <SectionHeading title="Realized sales" />
                <div className="account-preview-sale-head" aria-hidden="true">
                  <span>Sale</span>
                  <span>Shares</span>
                  <span>Sale price</span>
                  <span>Proceeds</span>
                  <span>FIFO basis</span>
                  <span>Estimated P/L</span>
                </div>
                <div className="account-preview-sale-list">
                  {sales.map((trade) => {
                    const tone =
                      trade.estimatedRealizedGain == null
                        ? 'muted'
                        : trade.estimatedRealizedGain >= 0
                          ? 'positive'
                          : 'negative'
                    return (
                      <div className="account-preview-sale-row" key={trade.id}>
                        <span className="table-asset">
                          <BrandMark
                            className="asset-mark"
                            fallback={stockMarkLabel(trade.ticker ?? '—')}
                            label={`${trade.ticker ?? 'Security'} logo`}
                            src={stockLogoUrl(trade.ticker ?? '—', externalLogosEnabled)}
                            style={{ backgroundColor: stockMarkColor(trade.ticker ?? '—') }}
                          />
                          <span>
                            <strong>{trade.ticker ?? 'Security'}</strong>
                            <small>{formatActivityDate(trade.date, data.updatedAt)}</small>
                          </span>
                        </span>
                        <span>
                          {trade.units?.toLocaleString('en-US', { maximumFractionDigits: 4 }) ??
                            '—'}
                        </span>
                        <span>{formatCurrency(trade.price)}</span>
                        <strong>{formatCurrency(Math.abs(trade.amount))}</strong>
                        <span>{formatCurrency(trade.realizedCostBasis)}</span>
                        <span className={tone}>
                          <strong>{formatCurrency(trade.estimatedRealizedGain)}</strong>
                          <small>
                            {trade.estimatedRealizedGain == null
                              ? 'Lot match unavailable'
                              : formatPercent(trade.estimatedRealizedGainPct)}
                          </small>
                        </span>
                      </div>
                    )
                  })}
                  {!sales.length ? (
                    <p className="overview-recent-empty">No imported sales this year.</p>
                  ) : null}
                </div>
              </Card>
            </div>
          ) : null}

          <Card className="account-preview-card account-activity-preview">
            <SectionHeading title={activityLink} />
            <ActivityList
              activities={activities.slice(
                0,
                isInvestment ? accountActivityPreviewLimit(positions.length, sales.length) : 3,
              )}
              referenceIso={data.updatedAt}
              emptyMessage="No imported account activity."
              compact
            />
          </Card>
        </div>
      </div>
    )
  }

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
            {account.institution}
            <br />
            {isInvestment ? historyLabel : 'Current provider balance'}
            {performance?.historyStart
              ? ` · from ${formatActivityDate(performance.historyStart, data.updatedAt)}`
              : ''}
          </span>
        </header>
        <div
          className={`account-metric-grid${!isInvestment && !isCredit ? ' account-income-grid' : ''}`}
        >
          {isInvestment ? (
            <>
              <Metric
                label={account.costBasisCoverage === 'partial' ? 'Known cost basis' : 'Cost basis'}
                value={formatCurrency(account.knownCostBasis)}
                detail={
                  missingBasis
                    ? `${missingBasis} positions missing basis or value`
                    : 'Provider-reported positions'
                }
              />
              <Metric
                label={
                  account.costBasisCoverage === 'partial'
                    ? 'Known unrealized P/L'
                    : 'Unrealized P/L'
                }
                value={formatCurrency(gain)}
                detail={formatPercent(account.knownUnrealizedGainPct)}
                tone={gainTone}
              />
              <Metric
                label="Estimated realized P/L"
                value={formatCurrency(realizedGain)}
                detail={realizedGain == null ? 'Reconciled lots unavailable' : 'Estimated FIFO'}
                tone={realizedGainTone}
              />
              <Metric
                label="Positions"
                value={positions.length}
                detail={`${trades.length} imported trades`}
              />
            </>
          ) : isCredit ? (
            <>
              <Metric label="Spending this month" value={formatCurrency(spending.total)} />
              <Metric label="Pending spending" value={formatCurrency(spending.pendingTotal)} />
              <Metric
                label="Largest purchase"
                value={formatCurrency(spending.biggest ? Math.abs(spending.biggest.amount) : null)}
                detail={spending.biggest?.merchant}
              />
              <Metric label="Imported transactions" value={transactions.length} />
            </>
          ) : (
            <>
              <Metric label="Imported transactions" value={transactions.length} />
              <Metric
                label="Pending"
                value={transactions.filter(({ pending }) => pending).length}
              />
              <Metric label="Currency" value={account.currency ?? 'Unknown'} />
            </>
          )}
        </div>
        {!isInvestment ? (
          <p className="account-realized-note">
            {isCredit
              ? 'Card balances are liabilities in net worth. History is reconstructed from fetched posted activity when coverage supports it.'
              : 'History is reconstructed from fetched posted activity and anchored to the current provider balance.'}
          </p>
        ) : null}
      </Card>

      <Card className="account-history-card account-detail-section">
        <SectionHeading
          title={isInvestment ? 'Performance' : isCredit ? 'Monthly spending' : 'Balance history'}
          detail={isCredit ? 'Card spending across the last six months.' : historyLabel}
        />
        {isCredit ? (
          <MonthlyBarChart data={monthlySpending} label={`${displayName} monthly spending`} />
        ) : account.value != null && history.length > 1 ? (
          <PerformanceChart
            data={chartData}
            netDeposits={netDeposits}
            benchmark={benchmark}
            value={account.value}
            events={chartEvents}
            referenceIso={data.updatedAt}
            startDate={startDate}
            selectedWindow={graphWindow}
            onWindowChange={onGraphWindowChange}
          />
        ) : (
          <div className="account-chart-empty">
            <strong>Historical series unavailable</strong>
            <span>
              Brief has a current provider balance, but no supported account-level history to plot.
            </span>
          </div>
        )}
      </Card>

      {isInvestment ? (
        <Card className="account-positions-card account-detail-section">
          <SectionHeading
            title="Holdings"
            detail={`${positions.length} imported ${positions.length === 1 ? 'position' : 'positions'} · Prices and P/L use committed provider data.`}
          />
          <PositionTable positions={positions} externalLogosEnabled={externalLogosEnabled} />
        </Card>
      ) : null}

      {!isCredit ? (
        <Card className="account-history-card account-income-card account-detail-section">
          <SectionHeading
            title="Income"
            detail={`Imported ${isInvestment ? 'dividends and interest' : 'interest'} in ${data.updatedAt.slice(0, 4)}.`}
          />
          <div
            className={`account-metric-grid account-income-grid${isInvestment ? '' : ' account-income-grid-single'}`}
          >
            {isInvestment ? (
              <>
                <Metric label="Dividends" value={formatCurrency(income.dividends)} />
                <Metric label="Interest" value={formatCurrency(income.interest)} />
                <Metric
                  label="Total investment income"
                  value={formatCurrency(
                    account.investmentIncomeYtd ?? income.dividends + income.interest,
                  )}
                  detail="Canonical account total"
                />
              </>
            ) : (
              <Metric label="Interest" value={formatCurrency(income.interest)} />
            )}
          </div>
        </Card>
      ) : null}

      {isInvestment ? (
        <Card className="account-realized-trades-card account-detail-section">
          <SectionHeading
            title="Realized sales"
            detail="FIFO estimates require reconciled purchase lots."
          />
          <div className="account-metric-grid account-sales-grid">
            <Metric
              label="Sale proceeds"
              value={formatCurrency(account.saleProceedsYtd)}
              detail="Proceeds, not profit"
            />
            <Metric label="Sales" value={account.salesYtd ?? 0} />
            <Metric
              label="Estimated realized P/L"
              value={formatCurrency(realizedGain)}
              detail={realizedGain == null ? 'Reconciled lots unavailable' : 'Estimated FIFO'}
              tone={realizedGainTone}
            />
          </div>
          <SalesTable sales={sales} externalLogosEnabled={externalLogosEnabled} />
          {account.salesYtd ? (
            <p className="account-realized-note">
              {realizedGain != null
                ? `Estimated with FIFO from imported executions${account.realizedGainCoverage === 'partial' ? '; some sales could not be matched' : ''}. Your broker’s tax result may differ.`
                : 'Realized P/L is unavailable because imported executions do not reconcile to enough purchase lots.'}
            </p>
          ) : null}
        </Card>
      ) : null}

      <Card className="account-transactions-card account-detail-section">
        <SectionHeading
          title="Latest activity"
          detail="Trades, income, transfers, fees, and account transactions."
        />
        <ActivityList
          activities={activities}
          referenceIso={data.updatedAt}
          emptyMessage="No imported account activity."
        />
      </Card>
    </div>
  )
}
