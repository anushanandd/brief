import { Link } from '@tanstack/react-router'
import { useMemo, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { ChartRangeSelector, MonthlyBarChart, PerformanceChart } from '../components/charts'
import { ValueHistoryEmptyState } from '../components/data-state'
import { ExpectedActivity } from '../components/expected-activity'
import { ChevronRight, Download } from '../components/icons'
import { PositionTable } from '../components/position-table'
import { SalesTable } from '../components/sales-table'
import { SnapTradeReference } from '../components/snaptrade-reference'
import {
  AnimatedCurrency,
  Button,
  Card,
  ChartChange,
  Metric,
  ScrollCueCard,
  SectionHeading,
} from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { useLiveFinance } from '../hooks/use-live-finance'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
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
import { formatCurrency, formatPercent, valueTone } from '../lib/format'
import { graphWindows } from '../lib/graph-preferences'
import { buildLiveChartData } from '../lib/live-chart'
import { getExternalLogosEnabled, transactionMarkKind } from '../lib/logos'
import type { Account, FinanceSnapshot, Transaction } from '../lib/schema'
import {
  buildMonthlySpendingHistory,
  buildSpendingView,
  sortTransactionsByRecency,
  transactionDateKey,
} from '../lib/spending'

const daySeconds = 24 * 60 * 60
const cents = (value: number) => Math.round(value * 100)
const analyticsRangeValues = ['week', 'month', 'quarter', 'year', 'all'] as const
export function accountTransactionSummary(
  transactions: Transaction[],
  referenceIso: string,
  windowSeconds: number,
) {
  const end = transactionDateKey(referenceIso, referenceIso)
  const start = windowSeconds
    ? new Date(Date.parse(`${end}T00:00:00Z`) - windowSeconds * 1_000).toISOString().slice(0, 10)
    : ''
  const posted = transactions.flatMap((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, referenceIso)
    return !transaction.pending && date && date <= end ? [{ transaction, date }] : []
  })
  const summarize = (items: typeof posted) => {
    let moneyIn = 0
    let moneyOut = 0
    let interest = 0
    let dividends = 0
    for (const { transaction } of items) {
      const amount = cents(transaction.amount)
      if (amount > 0) moneyIn += amount
      if (amount < 0) moneyOut -= amount
      const kind = transactionMarkKind(transaction)
      if (amount > 0 && kind === 'interest') interest += amount
      if (amount > 0 && kind === 'dividend') dividends += amount
    }
    return {
      moneyIn: moneyIn / 100,
      moneyOut: moneyOut / 100,
      netFlow: (moneyIn - moneyOut) / 100,
      interest: interest / 100,
      dividends: dividends / 100,
    }
  }
  const firstDate = posted.toSorted((left, right) => left.date.localeCompare(right.date))[0]?.date
  const days = windowSeconds
    ? Math.max(1, windowSeconds / daySeconds)
    : firstDate
      ? Math.max(1, (Date.parse(end) - Date.parse(firstDate)) / 86_400_000 + 1)
      : 1
  return {
    range: summarize(posted.filter(({ date }) => !start || date >= start)),
    allTime: summarize(posted),
    start,
    end,
    days,
  }
}

function sumKnown(values: Array<number | null | undefined>) {
  const known = values.filter((value): value is number => value != null)
  return known.length ? known.reduce((total, value) => total + value, 0) : null
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
  const finance = useFinance()
  const committed = finance.data ?? data
  const [exporting, setExporting] = useState(false)
  const [exportError, setExportError] = useState(false)
  const salesScrollRef = useViewportScroll(32)
  const activityScrollRef = useViewportScroll(32)
  const expectedScrollRef = useViewportScroll<HTMLUListElement>(32)
  const live = useLiveFinance()
  const accountNamesKey = JSON.stringify(getAccountDisplayNames())
  const accountDisplayNames = useMemo(
    () => parseAccountDisplayNames(accountNamesKey),
    [accountNamesKey],
  )
  const accountStartDates = getAccountStartDates()
  const isCombined = account.id === 'all' || account.type === 'combined'
  const committedAccount = committed.accounts.find(({ id }) => id === account.id)
  const startDate = accountStartDate(data, isCombined ? 'net-worth' : account.id, accountStartDates)
  const externalLogosEnabled = getExternalLogosEnabled()
  const displayName = accountDisplayName(account.id, account.name, accountDisplayNames)
  const relatedAccountIds = useMemo(
    () =>
      new Set(
        isCombined
          ? committed.accounts.filter(({ id }) => id !== 'all').map(({ id }) => id)
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
  const transactionSummary = useMemo(
    () => accountTransactionSummary(transactions, data.updatedAt, graphWindow),
    [data.updatedAt, graphWindow, transactions],
  )
  const isInvestment = isCombined || account.type === 'brokerage' || account.type === 'retirement'
  const showExpectedActivity = isCombined || !isInvestment
  const previewLayoutClass = !showExpectedActivity
    ? ' account-preview-grid-wide-primary'
    : account.type === 'cash'
      ? ' account-preview-grid-wide-recent'
      : ''
  const isCredit = account.type === 'credit'
  const performance = isCombined
    ? undefined
    : (isInvestment ? data.brokeragePerformance : data.accountBalanceHistory).find(
        ({ accountId }) => accountId === account.id,
      )
  const chartNow = Date.parse(live.valuationAsOf ?? data.updatedAt) / 1_000
  const now = Date.now() / 1_000
  const safeChartNow = Number.isFinite(chartNow) ? chartNow : now
  const marketSeries = live.marketSeries[isCombined ? 'net-worth' : account.id] ?? []
  const { chartData, chartChange, incomplete, historyIsAvailable } = accountValueChart(
    data,
    account,
    marketSeries,
    safeChartNow,
    graphWindow,
    now,
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
  const investmentAccounts = isCombined
    ? data.accounts.filter(({ type }) => type === 'brokerage' || type === 'retirement')
    : []
  const gain = isCombined
    ? sumKnown(investmentAccounts.map(({ knownUnrealizedGain }) => knownUnrealizedGain))
    : account.knownUnrealizedGain
  const gainTone = valueTone(gain)
  const realizedGain = isCombined
    ? sumKnown(investmentAccounts.map(({ estimatedRealizedGainYtd }) => estimatedRealizedGainYtd))
    : account.estimatedRealizedGainYtd
  const realizedGainTone = valueTone(realizedGain)
  const unrealizedLabel =
    isCombined || account.costBasisCoverage === 'partial'
      ? 'Known unrealized P/L'
      : 'Unrealized P/L'
  const rangeIndex = graphWindows.findIndex(({ secs }) => secs === graphWindow)
  const analyticsRange = analyticsRangeValues[Math.max(0, rangeIndex)]
  const groupedBalance = (types: string[]) => {
    const accounts = data.accounts.filter(({ id, type }) => id !== 'all' && types.includes(type))
    return accounts.length && accounts.every((item) => item.value != null)
      ? accounts.reduce((sum, item) => sum + item.value!, 0)
      : null
  }
  const cashBalance = groupedBalance(['cash'])
  const stockBalance = groupedBalance(['brokerage', 'retirement'])
  const rangeMarketChanges =
    performance?.performanceMethod === 'value-only'
      ? []
      : (performance?.points.flatMap(({ date, marketChange }) =>
          marketChange != null &&
          date <= transactionSummary.end &&
          (!transactionSummary.start || date >= transactionSummary.start)
            ? [marketChange]
            : [],
        ) ?? [])
  const rangeUnrealizedGain = rangeMarketChanges.length
    ? rangeMarketChanges.reduce((sum, value) => sum + cents(value), 0) / 100
    : null
  const estimatedInterestRate =
    account.value && account.value > 0
      ? (transactionSummary.range.interest / account.value) * (365 / transactionSummary.days) * 100
      : null
  const averageMonthlyFlow =
    (transactionSummary.allTime.netFlow / transactionSummary.days) * (365.2425 / 12)
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
  const activityPreview = (
    <ScrollCueCard
      className="account-preview-card account-activity-preview"
      scrollSelector=".account-activity-scroll"
    >
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
              exporting || !activities.some((item) => isCombined || item.accountId === account.id)
            }
            aria-busy={exporting}
            onClick={async () => {
              setExporting(true)
              setExportError(false)
              try {
                await downloadActivityCsv(activities, isCombined ? 'all' : account.id, displayName)
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
      <div
        ref={activityScrollRef}
        className="account-activity-scroll"
        role="region"
        aria-label="Recent account activity"
        tabIndex={0}
        data-keyboard-region
      >
        <ActivityList
          activities={activities}
          referenceIso={data.updatedAt}
          emptyMessage="No imported account activity."
          compact
          conciseTradeGain
        />
      </div>
    </ScrollCueCard>
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
                {isCombined && data.netWorthProvisional ? ' · provisional balance' : ''}
                {account.balanceSource === 'unavailable' ? ' · value unavailable' : ''}
              </h2>
              <div className="home-balance-value">
                <AnimatedCurrency className="hero-number" value={account.value} />
              </div>
            </div>
            {committedAccount?.id.startsWith('snaptrade:') || (!isCredit && historyIsAvailable) ? (
              <div className="chart-header-aside">
                {!isCredit && historyIsAvailable ? (
                  <ChartRangeSelector value={graphWindow} onValueChange={onGraphWindowChange} />
                ) : null}
                <SnapTradeReference account={committedAccount} />
                {account.id.startsWith('snaptrade:') && marketSeries.length ? (
                  <small className="snaptrade-reference">
                    SnapTrade daily values · Alpaca intraday projection
                  </small>
                ) : null}
              </div>
            ) : null}
            {!isCredit && historyIsAvailable ? (
              <div className="chart-summary-row">
                <ChartChange amount={chartChange.change} percent={chartChange.percent} />
              </div>
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
                    value={
                      isCombined && data.netWorthProvisional
                        ? (chartData.at(-1)?.value ?? account.value)
                        : account.value
                    }
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
          <SectionHeading title="Account details" />
          <div className="account-summary-metrics">
            {isCombined ? (
              <>
                <Metric label="Cash" value={formatCurrency(cashBalance)} />
                <Metric label="Stock" value={formatCurrency(stockBalance)} />
                <Metric
                  label="Interest"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'interest', range: analyticsRange }}
                    >
                      {formatCurrency(transactionSummary.range.interest)}
                    </Link>
                  }
                />
                <Metric
                  label="Dividends"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'dividends', range: analyticsRange }}
                    >
                      {formatCurrency(transactionSummary.range.dividends)}
                    </Link>
                  }
                />
                <Metric label={unrealizedLabel} value={formatCurrency(gain)} tone={gainTone} />
                <Metric
                  label="Estimated realized P/L"
                  value={formatCurrency(realizedGain)}
                  tone={realizedGainTone}
                />
              </>
            ) : isInvestment ? (
              <>
                <Metric
                  label="Market change"
                  value={formatCurrency(rangeUnrealizedGain)}
                  tone={valueTone(rangeUnrealizedGain)}
                />
                <Metric label="Cash" value={formatCurrency(account.cashValue)} />
                <Metric
                  label={
                    unrealizedLabel === 'Known unrealized P/L'
                      ? 'Known total unrealized'
                      : 'Total unrealized'
                  }
                  value={formatCurrency(gain)}
                  tone={gainTone}
                />
                <Metric
                  label="Estimated realized P/L"
                  value={formatCurrency(realizedGain)}
                  tone={realizedGainTone}
                />
                <Metric
                  label="Dividends"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'dividends', range: analyticsRange, account: account.id }}
                    >
                      {formatCurrency(transactionSummary.range.dividends)}
                    </Link>
                  }
                />
                <Metric
                  label="Interest"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'interest', range: analyticsRange, account: account.id }}
                    >
                      {formatCurrency(transactionSummary.range.interest)}
                    </Link>
                  }
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
                />
                <Metric label="Transactions" value={transactions.length} />
              </>
            ) : (
              <>
                <Metric label="Money in" value={formatCurrency(transactionSummary.range.moneyIn)} />
                <Metric
                  label="Money out"
                  value={formatCurrency(transactionSummary.range.moneyOut)}
                />
                <Metric
                  label="Interest"
                  value={
                    <Link
                      className="metric-link"
                      to="/analytics"
                      search={{ chart: 'interest', range: analyticsRange, account: account.id }}
                    >
                      {formatCurrency(transactionSummary.range.interest)}
                    </Link>
                  }
                />
                <Metric label="Est. interest rate" value={formatPercent(estimatedInterestRate)} />
                <Metric
                  label="Net flow"
                  value={formatCurrency(transactionSummary.range.netFlow)}
                  tone={valueTone(transactionSummary.range.netFlow)}
                />
                <Metric
                  label="All-time monthly avg."
                  value={formatCurrency(averageMonthlyFlow)}
                  tone={valueTone(averageMonthlyFlow)}
                />
              </>
            )}
          </div>
        </Card>
      </div>

      <div className={`account-preview-grid${previewLayoutClass}`}>
        {isInvestment ? (
          <div className="account-investment-preview-column">
            <Card className="account-preview-card account-holdings-preview">
              <PositionTable
                title={holdingsLink}
                positions={positions.slice(0, 3)}
                externalLogosEnabled={externalLogosEnabled}
              />
            </Card>
            <ScrollCueCard
              className="account-preview-card account-sales-preview"
              scrollSelector=".financial-table-scroll"
            >
              <SalesTable
                sales={sales}
                externalLogosEnabled={externalLogosEnabled}
                referenceIso={data.updatedAt}
                scrollRef={salesScrollRef}
              />
            </ScrollCueCard>
          </div>
        ) : null}

        <div className="account-activity-preview-column account-recent-preview">
          {activityPreview}
        </div>
        {showExpectedActivity ? (
          <div className="account-activity-preview-column account-expected-column">
            <ExpectedActivity
              data={committed}
              account={isCombined ? undefined : account.id}
              className="account-preview-card account-expected-preview"
              scrollRef={expectedScrollRef}
            />
          </div>
        ) : null}
      </div>
    </div>
  )
}
