import { Link } from '@tanstack/react-router'
import { useMemo, useRef } from 'react'

import {
  cashMetrics,
  creditMetrics,
  investmentMetrics,
  ReportMetric,
} from '../components/account-metrics'
import { ActivityList } from '../components/activity-list'
import { AllAccountsOverview } from '../components/all-accounts-overview'
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
import { useCsvExport } from '../hooks/use-csv-export'
import { useFinance } from '../hooks/use-finance'
import { useLiveFinance } from '../hooks/use-live-finance'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { accountDisplayName } from '../lib/account-names'
import { accountStartDate } from '../lib/account-start-dates'
import { accountTransactionSummary, rangeInvestmentFlows, sumKnown } from '../lib/account-summary'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities } from '../lib/activity'
import { buildChartEventGroups } from '../lib/chart-events'
import { accountValueChart, withoutSpendingCardPoints } from '../lib/dashboard-account-views'
import { financeSettings } from '../lib/finance-settings'
import { formatCurrency, valueTone } from '../lib/format'
import { chartPeriod } from '../lib/graph-preferences'
import { buildLiveChartData } from '../lib/live-chart'
import { getExternalLogosEnabled } from '../lib/logos'
import { sumCents } from '../lib/money'
import type { Account, FinanceSnapshot } from '../lib/schema'
import {
  buildMonthlySpendingHistory,
  buildSpendingView,
  sortTransactionsByRecency,
} from '../lib/spending'

// The figures an account shows before its Details disclosure.
const primaryMetricCount = 4
// The card previews; the heading opens the account's full history in Activity.
const recentActivityLimit = 8

export function AccountOverview({
  account,
  data,
  graphWindow,
  onGraphWindowChange,
  excludedActivityAccountId,
}: {
  account: Account
  data: FinanceSnapshot
  graphWindow: number
  onGraphWindowChange: (seconds: number) => void
  excludedActivityAccountId?: string
}) {
  const finance = useFinance()
  const committed = finance.data ?? data
  const csv = useCsvExport()
  const salesScrollRef = useViewportScroll(32)
  const expectedScrollRef = useViewportScroll<HTMLUListElement>(32)
  const live = useLiveFinance()
  const settings = financeSettings(committed)
  const accountDisplayNames = settings.accountNames
  const accountStartDates = settings.accountStartDates
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
      ).filter(({ accountId }) => accountId !== excludedActivityAccountId),
    [
      accountDisplayNames,
      committed.updatedAt,
      excludedActivityAccountId,
      externalLogosEnabled,
      trades,
      transactions,
    ],
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
  const marketSeries = isCombined
    ? withoutSpendingCardPoints(data, live.marketSeries['net-worth'] ?? [])
    : (live.marketSeries[account.id] ?? [])
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
  const realizedGain = isCombined
    ? sumKnown(investmentAccounts.map(({ estimatedRealizedGainYtd }) => estimatedRealizedGainYtd))
    : account.estimatedRealizedGainYtd
  const unrealizedLabel =
    isCombined || account.costBasisCoverage === 'partial'
      ? 'Known unrealized P/L'
      : 'Unrealized P/L'
  const period = chartPeriod(graphWindow)
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
  const investmentFlows = rangeInvestmentFlows(
    isCombined
      ? data.brokeragePerformance.filter(({ accountId }) =>
          investmentAccounts.some(({ id }) => id === accountId),
        )
      : performance && isInvestment
        ? [performance]
        : [],
    transactionSummary.start,
    transactionSummary.end,
  )
  const rangeUnrealizedGain = rangeMarketChanges.length ? sumCents(rangeMarketChanges) : null
  const holdingsLink = (
    <Link className="section-heading-link" to="/holdings" data-command="navigation.open./holdings">
      Holdings <ChevronRight size={14} aria-hidden="true" />
    </Link>
  )
  const activityLink = (
    <Link
      className="section-heading-link"
      to="/activities"
      search={isCombined ? undefined : { account: account.id }}
      data-command="navigation.open./activities"
    >
      Recent activity <ChevronRight size={14} aria-hidden="true" />
    </Link>
  )
  const canExport =
    !csv.exporting && activities.some((item) => isCombined || item.accountId === account.id)
  const exportCsv = async () => {
    if (canExport) await csv.run(activities, isCombined ? 'all' : account.id, displayName)
  }
  const exportRef = useRef(exportCsv)
  exportRef.current = exportCsv
  const detailsRef = useRef<HTMLDetailsElement>(null)
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'account.export',
          title: `Download ${displayName} activity as CSV`,
          search: 'export csv download spreadsheet account',
          group: 'Actions',
          scope: 'page',
          run: () => void exportRef.current(),
        },
        // All accounts shows every figure, so there's nothing to disclose.
        ...(isCombined
          ? []
          : ([
              {
                id: 'account.details.toggle',
                title: `Show or hide ${displayName} details`,
                search: 'account details more figures apr limit interest',
                group: 'View',
                scope: 'page',
                bindings: [{ sequence: ['d'], label: 'D' }],
                run: () => {
                  const details = detailsRef.current
                  if (details) details.open = !details.open
                },
              },
            ] satisfies AppAction[])),
      ],
      [displayName, isCombined],
    ),
  )
  const activityPreview = (
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
            data-command="account.export"
            disabled={!canExport}
            aria-busy={csv.exporting}
            onClick={() => void exportCsv()}
          >
            <Download size={16} aria-hidden="true" />
          </Button>
        }
      />
      {csv.failed ? (
        <p role="alert">CSV could not be saved. Try again with a new filename.</p>
      ) : null}
      <div role="region" aria-label="Recent account activity" tabIndex={0} data-keyboard-region>
        <ActivityList
          activities={activities.slice(0, recentActivityLimit)}
          referenceIso={data.updatedAt}
          emptyMessage="No imported account activity."
          conciseTradeGain
        />
      </div>
    </Card>
  )

  const chartCard = (
    <Card className="account-overview-chart-card net-worth-card brokerage-performance-card">
      <header className="home-balance-header">
        <div className="home-balance-main">
          <h2 className="balance-label">
            {displayName}
            {incomplete ? ' · known USD balances only' : ''}
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
        {isCredit ? null : (
          <div className="chart-summary-row">
            <ChartChange
              amount={historyIsAvailable ? chartChange.change : null}
              percent={historyIsAvailable ? chartChange.percent : null}
            />
          </div>
        )}
      </header>

      <div className="brokerage-chart-viewport">
        <div key={account.id} className="brokerage-chart-slide">
          <div className="brokerage-chart-content">
            {isCredit ? (
              <MonthlyBarChart data={monthlySpending} label={`${displayName} monthly spending`} />
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
  )

  // Each account type leads with the figures that decide something; the rest stay one
  // disclosure away, so every collected field remains reachable without a wall of tiles.
  const metrics = isInvestment
    ? investmentMetrics({
        account,
        summary: transactionSummary,
        range: period,
        rangeGain: rangeUnrealizedGain,
        contributions: investmentFlows.contributions,
        unrealized: {
          value: gain,
          label:
            unrealizedLabel === 'Known unrealized P/L'
              ? 'Known total unrealized'
              : 'Total unrealized',
        },
        realized: realizedGain,
      })
    : isCredit
      ? creditMetrics({
          account,
          spending,
          transactionCount: transactions.length,
          card: data.insights?.credit.cards.find(({ accountId }) => accountId === account.id),
        })
      : cashMetrics({
          account,
          summary: transactionSummary,
          range: period,
          interest: data.insights?.interest.find(({ accountId }) => accountId === account.id),
        })
  const primaryMetrics = metrics.slice(0, primaryMetricCount)
  const moreMetrics = metrics.slice(primaryMetricCount)

  // Investment and income figures across every account. Cash and stock totals are in "Made of".
  const combinedDetails = isCombined ? (
    <Card className="account-overview-summary-card">
      <SectionHeading title="Account details" />
      <div className="account-summary-metrics">
        <ReportMetric
          chart="interest"
          label="Interest"
          value={transactionSummary.range.interest}
          range={period}
        />
        <ReportMetric
          chart="dividends"
          label="Dividends"
          value={transactionSummary.range.dividends}
          range={period}
        />
        <Metric label={unrealizedLabel} value={formatCurrency(gain)} tone={valueTone(gain)} />
        <Metric
          label="Estimated realized P/L"
          value={formatCurrency(realizedGain)}
          tone={valueTone(realizedGain)}
        />
        <Metric label="Net contributions" value={formatCurrency(investmentFlows.contributions)} />
        <Metric
          label="Investment gain"
          value={formatCurrency(investmentFlows.marketChange)}
          tone={valueTone(investmentFlows.marketChange)}
        />
      </div>
    </Card>
  ) : null

  if (isCombined)
    return (
      <AllAccountsOverview
        data={committed}
        // The saved spending card belongs to Money; Accounts lists everything else.
        accounts={data.accounts.filter(
          ({ id }) => id !== 'all' && id !== excludedActivityAccountId,
        )}
        graphWindow={graphWindow}
        chartCard={chartCard}
        activityPreview={activityPreview}
        details={combinedDetails}
      />
    )

  return (
    <div className="account-overview-dashboard">
      <div className="account-overview-primary-grid">
        {chartCard}

        <Card className="account-overview-summary-card">
          <SectionHeading
            title="Account details"
            detail={
              [account.officialName, account.holderCategory === 'business' ? 'Business' : '']
                .filter(Boolean)
                .join(' · ') || undefined
            }
          />
          <div className="account-summary-metrics">{primaryMetrics}</div>
          {moreMetrics.length ? (
            <details ref={detailsRef} className="account-details-more">
              <summary data-command="account.details.toggle">
                Details <small>{moreMetrics.length}</small>
              </summary>
              <div className="account-summary-metrics">{moreMetrics}</div>
            </details>
          ) : null}
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
