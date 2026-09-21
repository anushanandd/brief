import { Link } from '@tanstack/react-router'
import { ChevronRight, Download } from 'lucide-react'
import { useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { MonthlyBarChart, PerformanceChart, PerformanceChartControls } from '../components/charts'
import { ValueHistoryEmptyState } from '../components/data-state'
import { PositionTable } from '../components/position-table'
import { SalesTable } from '../components/sales-table'
import { AnimatedCurrency, Button, Card, Change, Metric, SectionHeading } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { useLiveFinance } from '../hooks/use-live-finance'
import {
  accountDisplayName,
  getAccountDisplayNames,
  parseAccountDisplayNames,
} from '../lib/account-name-preferences'
import { accountStartDate, getAccountStartDates } from '../lib/account-start-date-preferences'
import { buildActivities } from '../lib/activity'
import { downloadActivityCsv } from '../lib/activity-csv'
import { buildChartEventGroups } from '../lib/chart-events'
import { accountValueChart } from '../lib/dashboard-account-views'
import { formatCurrency, formatPercent, valueTone, formatActivityName } from '../lib/format'
import { buildLiveChartData } from '../lib/live-chart'
import { getExternalLogosEnabled, transactionMarkKind } from '../lib/logos'
import type { Account, FinanceSnapshot, Transaction } from '../lib/schema'
import {
  buildMonthlySpendingHistory,
  buildSpendingView,
  sortTransactionsByRecency,
  transactionDateKey,
} from '../lib/spending'

export function accountActivityPreviewLimit(holdingCount: number, saleCount: number) {
  const holdingRowsHeight = holdingCount ? Math.min(holdingCount, 3) * 64 : 124
  const saleRowsHeight = saleCount ? saleCount * 64 : 124
  return Math.max(1, Math.floor((109 + holdingRowsHeight + saleRowsHeight) / 64))
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

export function AccountOverview({
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
  const committed = useFinance().data ?? data
  const [exporting, setExporting] = useState(false)
  const [exportError, setExportError] = useState(false)
  const live = useLiveFinance()
  const accountNamesKey = JSON.stringify(getAccountDisplayNames())
  const accountDisplayNames = useMemo(
    () => parseAccountDisplayNames(accountNamesKey),
    [accountNamesKey],
  )
  const accountStartDates = getAccountStartDates()
  const isCombined = account.id === 'all' || account.type === 'combined'
  const startDate = accountStartDate(data, isCombined ? 'net-worth' : account.id, accountStartDates)
  const externalLogosEnabled = getExternalLogosEnabled()
  const displayName = accountDisplayName(account.id, account.name, accountDisplayNames)
  const relatedAccountIds = useMemo(
    () =>
      new Set(
        isCombined
          ? committed.accounts
              .filter(({ id, type }) => id !== 'all' && type !== 'credit')
              .map(({ id }) => id)
          : [
              account.id,
              ...Object.entries(committed.accountLinks ?? {}).flatMap(([plaidId, snaptradeId]) =>
                snaptradeId === account.id ? [plaidId] : [],
              ),
            ],
      ),
    [account.id, committed.accountLinks, committed.accounts, isCombined],
  )
  const transactions = useMemo(
    () =>
      sortTransactionsByRecency(
        committed.transactions.filter(
          ({ accountId }) => accountId && relatedAccountIds.has(accountId),
        ),
        data.updatedAt,
      ),
    [committed.transactions, data.updatedAt, relatedAccountIds],
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
      buildActivities(
        { updatedAt: committed.updatedAt, transactions, trades },
        accountDisplayNames,
        externalLogosEnabled,
      ),
    [accountDisplayNames, committed.updatedAt, externalLogosEnabled, trades, transactions],
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
  const chartNow = Date.parse(live.valuationAsOf ?? data.updatedAt) / 1_000
  const safeChartNow = Number.isFinite(chartNow) ? chartNow : Date.now() / 1_000
  const { chartData, chartChange, incomplete, historyIsAvailable } = accountValueChart(
    data,
    account,
    live.marketSeries[isCombined ? 'net-worth' : account.id] ?? [],
    safeChartNow,
    graphWindow,
    Date.now() / 1_000,
    startDate,
  )
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
  const chartEvents = useMemo(
    () =>
      isInvestment
        ? buildChartEventGroups(committed, isCombined ? 'net-worth' : account.id, graphWindow)
        : [],
    [committed, isInvestment, isCombined, account.id, graphWindow],
  )
  const gain = account.knownUnrealizedGain
  const gainTone = valueTone(gain)
  const realizedGain = account.estimatedRealizedGainYtd
  const realizedGainTone = valueTone(realizedGain)
  const basisLabel = account.costBasisCoverage === 'partial' ? 'Known cost basis' : 'Cost basis'
  const unrealizedLabel =
    account.costBasisCoverage === 'partial' ? 'Known unrealized P/L' : 'Unrealized P/L'

  const accountDetailTitle = isCombined ? 'All accounts' : 'Account details'
  const holdingsLink = (
    <Link className="section-heading-link" to="/holdings">
      Holdings <ChevronRight size={14} aria-hidden="true" />
    </Link>
  )
  const activityLink = (
    <Link
      className="section-heading-link"
      to="/activities"
      search={isCombined ? undefined : { account: account.id }}
    >
      Recent activity <ChevronRight size={14} aria-hidden="true" />
    </Link>
  )

  return (
    <div className="account-overview-dashboard">
      <div className="account-overview-primary-grid">
        <Card className="account-overview-chart-card net-worth-card brokerage-performance-card">
          <header className="home-balance-header">
            <div className="home-balance-main">
              <h2 className="balance-label">
                {displayName}
                {incomplete ? ' · known USD balances only' : ''}
              </h2>
              <div className="home-balance-value">
                <AnimatedCurrency className="hero-number" value={account.value} />
              </div>
              {!isCredit && historyIsAvailable ? (
                <div className="chart-summary-row">
                  <div className="hero-change">
                    <span className={valueTone(chartChange.change)}>
                      {formatCurrency(chartChange.change)}
                    </span>
                    <Change value={chartChange.percent} />
                  </div>
                </div>
              ) : null}
            </div>
            {!isCredit && historyIsAvailable ? (
              <PerformanceChartControls
                value={graphWindow}
                onValueChange={onGraphWindowChange}
                netDeposits={netDeposits}
                benchmark={benchmark}
              />
            ) : null}
          </header>

          <div className="brokerage-chart-viewport">
            <div key={account.id} className="brokerage-chart-slide">
              <div className="brokerage-chart-content">
                {isCredit ? (
                  <MonthlyBarChart
                    data={monthlySpending}
                    label={`${displayName} monthly spending`}
                  />
                ) : historyIsAvailable && account.value != null ? (
                  <PerformanceChart
                    data={chartData}
                    netDeposits={netDeposits}
                    benchmark={benchmark}
                    value={account.value}
                    events={chartEvents}
                    referenceIso={data.updatedAt}
                    startDate={startDate}
                    selectedWindow={graphWindow}
                  />
                ) : (
                  <ValueHistoryEmptyState
                    incomplete={incomplete}
                    valueAvailable={account.value != null}
                  />
                )}
              </div>
            </div>
          </div>
        </Card>

        <Card className="account-overview-summary-card">
          <SectionHeading title={accountDetailTitle} />
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
                <Metric label={basisLabel} value={formatCurrency(account.knownCostBasis)} />
                <Metric
                  label={unrealizedLabel}
                  value={formatCurrency(gain)}
                  detail={formatPercent(account.knownUnrealizedGainPct)}
                  tone={gainTone}
                />
                <Metric
                  label="Dividends YTD"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'dividends', range: 'year', account: account.id }}
                    >
                      {formatCurrency(income.dividends)}
                    </Link>
                  }
                />
                <Metric
                  label="Interest YTD"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'interest', range: 'year', account: account.id }}
                    >
                      {formatCurrency(income.interest)}
                    </Link>
                  }
                />
                <Metric
                  label="Sale proceeds YTD"
                  value={formatCurrency(account.saleProceedsYtd)}
                  detail={`${sales.length} imported ${sales.length === 1 ? 'sale' : 'sales'}`}
                />
                <Metric
                  label="Estimated realized P/L YTD"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'realized', range: 'year', account: account.id }}
                    >
                      {formatCurrency(realizedGain)}
                    </Link>
                  }
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
                  detail={
                    spending.biggest ? formatActivityName(spending.biggest.merchant) : undefined
                  }
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
                <Metric
                  label="Interest YTD"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'interest', range: 'year', account: account.id }}
                    >
                      {formatCurrency(income.interest)}
                    </Link>
                  }
                />
              </>
            )}
          </div>
        </Card>
      </div>

      <div className={`account-preview-grid${isInvestment ? '' : ' account-preview-grid-simple'}`}>
        {isInvestment ? (
          <div className="account-investment-preview-column">
            <Card className="account-preview-card account-holdings-preview">
              <PositionTable
                title={holdingsLink}
                positions={positions.slice(0, 3)}
                externalLogosEnabled={externalLogosEnabled}
              />
            </Card>
            <Card className="account-preview-card account-sales-preview">
              <SalesTable
                sales={sales}
                externalLogosEnabled={externalLogosEnabled}
                referenceIso={data.updatedAt}
              />
            </Card>
          </div>
        ) : null}

        <Card className="account-preview-card account-activity-preview">
          <SectionHeading
            title={activityLink}
            action={
              <Button
                variant="ghost"
                size="icon"
                className="icon-only-subtle"
                aria-label="Download account activity as CSV"
                title={`Download ${displayName} activity as CSV`}
                disabled={
                  exporting ||
                  !activities.some((item) => isCombined || item.accountId === account.id)
                }
                aria-busy={exporting}
                onClick={async () => {
                  setExporting(true)
                  setExportError(false)
                  try {
                    await downloadActivityCsv(
                      activities,
                      isCombined ? 'all' : account.id,
                      displayName,
                    )
                  } catch {
                    setExportError(true)
                  } finally {
                    setExporting(false)
                  }
                }}
              >
                <Download size={16} aria-hidden="true" />
              </Button>
            }
          />
          {exportError ? (
            <p role="alert">CSV could not be saved. Try again with a new filename.</p>
          ) : null}
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
