import { Link } from '@tanstack/react-router'
import { useMemo, useRef } from 'react'

import {
  cashMetrics,
  creditMetrics,
  investmentMetrics,
  ReportMetric,
} from '../components/account-metrics'
import { AccountStatements } from '../components/account-statements'
import { ActivityList } from '../components/activity-list'
import { AllAccountsOverview } from '../components/all-accounts-overview'
import { ChartRangeSelector, MonthlyBarChart, PerformanceChart } from '../components/charts'
import { ValueHistoryEmptyState } from '../components/data-state'
import { ExpectedActivity } from '../components/expected-activity'
import { ExternalLink } from '../components/external-link'
import { ArrowUpRight, ChevronRight, Download } from '../components/icons'
import { PositionTable } from '../components/position-table'
import { SalesTable } from '../components/sales-table'
import { SettingsLink } from '../components/settings-dialog'
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
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities } from '../lib/activity'
import { openExternalUrl } from '../lib/api'
import { buildChartEventGroups } from '../lib/chart-events'
import { accountValueChart } from '../lib/dashboard-account-views'
import { financeSettings } from '../lib/finance-settings'
import { formatCurrency, formatShares, formatShortDay, valueTone } from '../lib/format'
import { chartPeriod } from '../lib/graph-preferences'
import { buildLiveChartData } from '../lib/live-chart'
import { getExternalLogosEnabled } from '../lib/logos'
import type { Account, FinanceSnapshot } from '../lib/schema'
import {
  currentSpendingPeriod,
  monthlySpendingHistory,
  sortTransactionsByRecency,
} from '../lib/spending'
import { reconciliationSummary, reviewCount } from '../lib/statement-import'

// The figures an account shows before its Details disclosure.
const primaryMetricCount = 4
// The card previews; the heading opens the account's full history in Activity.
const recentActivityLimit = 8

export function AccountOverview({
  account,
  data,
  graphWindow,
  onGraphWindowChange,
  spendingAccountIds = [],
}: {
  account: Account
  data: FinanceSnapshot
  graphWindow: number
  onGraphWindowChange: (seconds: number) => void
  /** Cards Money presents; All accounts lists them apart and leaves them out of its totals. */
  spendingAccountIds?: string[]
}) {
  const finance = useFinance()
  const committed = finance.data ?? data
  const csv = useCsvExport()
  const salesScrollRef = useViewportScroll(32)
  const expectedScrollRef = useViewportScroll<HTMLUListElement>(32)
  const live = useLiveFinance()
  const settings = financeSettings(committed)
  const accountDetails = settings.accountDetails?.[account.id]
  const website = accountDetails?.website
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
      ).filter(
        ({ accountId }) => !isCombined || !accountId || !spendingAccountIds.includes(accountId),
      ),
    [
      accountDisplayNames,
      committed.updatedAt,
      spendingAccountIds,
      isCombined,
      externalLogosEnabled,
      trades,
      transactions,
    ],
  )
  const spending = currentSpendingPeriod(data.insights, account.id, 'month')
  const largestSpend = data.transactions.find(({ id }) => id === spending?.largest)
  const monthlySpending = useMemo(
    () => monthlySpendingHistory(data.insights, account.id),
    [account.id, data.insights],
  )
  const period = chartPeriod(graphWindow)
  // What moved through the account over the period, as Rust summed it.
  const transactionSummary = committed.insights?.flows.find(
    (flows) => flows.accountId === (isCombined ? 'all' : account.id) && flows.window === period,
  ) ?? {
    moneyIn: 0,
    moneyOut: 0,
    netFlow: 0,
    interest: 0,
    dividends: 0,
    contributions: null,
    marketChange: null,
  }
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
    ? (live.marketSeries.accounts ?? [])
    : (live.marketSeries[account.id] ?? [])
  const { chartData, chartChange, incomplete, historyIsAvailable } = accountValueChart(
    data,
    account,
    marketSeries,
    safeChartNow,
    graphWindow,
    live.chartChanges,
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
  // Across every account, the gains Rust summed at the values shown.
  const rollup = data.insights?.rollup
  const gain = isCombined ? (rollup?.unrealized ?? null) : account.knownUnrealizedGain
  const realizedGain = isCombined ? (rollup?.realizedYtd ?? null) : account.estimatedRealizedGainYtd
  const unrealizedLabel =
    isCombined || account.costBasisCoverage === 'partial'
      ? 'Known unrealized P/L'
      : 'Unrealized P/L'
  const investmentFlows = {
    contributions: transactionSummary.contributions,
    marketChange: transactionSummary.marketChange,
  }
  // One account's own market movement over the period; across accounts it reads as one figure.
  const rangeUnrealizedGain = isCombined ? null : transactionSummary.marketChange
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
                bindings: [{ sequence: ['i'], label: 'I' }],
                run: () => {
                  const details = detailsRef.current
                  if (details) details.open = !details.open
                },
              },
            ] satisfies AppAction[])),
        ...(website
          ? ([
              {
                id: 'account.website.open',
                title: `Open ${displayName}'s website`,
                search: 'account website bank login manage online',
                group: 'Actions',
                scope: 'page',
                run: () => void openExternalUrl(website).catch(() => undefined),
              },
            ] satisfies AppAction[])
          : []),
      ],
      [displayName, isCombined, website],
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
            {account.balanceSource === 'estimated' && account.balanceAsOf
              ? ` · Est. since ${formatShortDay(account.balanceAsOf)}`
              : ''}
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
                observedOn={account.observedOn}
                estimatedAfter={
                  account.balanceSource === 'estimated'
                    ? (account.balanceAsOf ?? undefined)
                    : undefined
                }
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
          largest: largestSpend,
          transactionCount: transactions.length,
          card: data.insights?.credit.cards.find(({ accountId }) => accountId === account.id),
        })
      : cashMetrics({
          account,
          summary: transactionSummary,
          range: period,
          interest: data.insights?.interest.find(({ accountId }) => accountId === account.id),
          apy: accountDetails?.apy,
        })
  // Statement reconciliation leads when something needs a look, and otherwise waits in Details.
  const reconciled = committed.statementReconciliation?.find(
    ({ accountId }) => accountId === account.id,
  )
  const statementMetric = reconciled ? (
    <StatementMetric key="statements" reconciled={reconciled} />
  ) : null
  const toReview = reconciled ? reviewCount(reconciled) : 0
  // A manual account's statement funds have no market price, so they lead its details rather
  // than appear among holdings; how the value is reached follows them.
  const positionMetrics = (account.statementPositions ?? []).map((position) => (
    <Metric
      key={`position:${position.name}`}
      label={position.name}
      value={
        <>
          {formatCurrency(position.value)}
          <small className="metric-detail">
            {formatShares(position.units)} units at {formatCurrency(position.price)} ·{' '}
            {formatShortDay(position.pricedOn)}
          </small>
        </>
      }
    />
  ))
  const valuationMetric =
    account.balanceSource === 'estimated' && account.balanceAsOf ? (
      <Metric
        key="valuation"
        label="Valuation"
        value={
          <>
            Est. since {formatShortDay(account.balanceAsOf)}
            <small className="metric-detail">moved with the S&amp;P 500</small>
          </>
        }
      />
    ) : null
  const withPositions = [
    ...positionMetrics,
    ...(valuationMetric ? [valuationMetric] : []),
    ...metrics,
  ]
  const ordered = statementMetric
    ? toReview
      ? [statementMetric, ...withPositions]
      : [...withPositions, statementMetric]
    : withPositions
  const primaryMetrics = ordered.slice(0, primaryMetricCount)
  const moreMetrics = ordered.slice(primaryMetricCount)

  // Spending cards belong to Money: All accounts lists them apart, outside its totals.
  const listedAccounts = isCombined
    ? data.accounts.filter(({ id }) => id !== 'all' && !spendingAccountIds.includes(id))
    : []
  const spendingCards = isCombined
    ? data.accounts.filter(({ id }) => spendingAccountIds.includes(id))
    : []
  const sheet = rollup?.sheet ?? { assets: 0, liabilities: 0, complete: false }
  // What the accounts own and owe, then investment and income figures across every account.
  // Group totals are in "Balances".
  const combinedDetails = isCombined ? (
    <Card className="account-overview-summary-card">
      <SectionHeading title="Account details" />
      <div className="account-summary-metrics">
        <Metric
          label={sheet.complete ? 'Assets' : 'Known assets'}
          value={formatCurrency(sheet.assets)}
        />
        <Metric
          label={sheet.complete ? 'Liabilities' : 'Known liabilities'}
          value={formatCurrency(sheet.liabilities)}
          tone={sheet.liabilities < 0 ? 'negative' : undefined}
        />
        <ReportMetric
          chart="interest"
          label="Interest"
          value={transactionSummary.interest}
          range={period}
        />
        <ReportMetric
          chart="dividends"
          label="Dividends"
          value={transactionSummary.dividends}
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
        accounts={listedAccounts}
        spendingCards={spendingCards}
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
              [
                account.officialName,
                account.holderCategory === 'business' ? 'Business' : '',
                spendingAccountIds.includes(account.id) ? 'Spending card, not in totals' : '',
              ]
                .filter(Boolean)
                .join(' · ') || undefined
            }
            action={
              accountDetails?.website ? (
                <ExternalLink
                  className="section-heading-link"
                  href={accountDetails.website}
                  data-command="account.website.open"
                >
                  Website
                  <ArrowUpRight size={12} aria-hidden="true" />
                </ExternalLink>
              ) : undefined
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
          <AccountStatements accountId={account.id} />
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

/** How the account's imported statements compare with Plaid; opens Statement history. */
function StatementMetric({
  reconciled,
}: {
  reconciled: NonNullable<FinanceSnapshot['statementReconciliation']>[number]
}) {
  const { headline, detail, review } = reconciliationSummary(reconciled)
  return (
    <Metric
      label="Statements"
      value={
        <SettingsLink target="statement-history" className={review ? 'warning' : undefined}>
          {headline}
          <small className="metric-detail">{detail}</small>
        </SettingsLink>
      }
    />
  )
}
