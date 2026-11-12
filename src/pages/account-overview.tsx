import { Link } from '@tanstack/react-router'
import { useMemo, useRef, type ReactNode } from 'react'

import { ActivityList } from '../components/activity-list'
import { AccountsLayout } from '../components/all-accounts-overview'
import { ChartRangeSelector, MonthlyBarChart, PerformanceChart } from '../components/charts'
import { ValueHistoryEmptyState } from '../components/data-state'
import { ExpectedActivity } from '../components/expected-activity'
import { ChevronRight, Download } from '../components/icons'
import { useObjectDetailsAction } from '../components/object-details'
import { PositionTable } from '../components/position-table'
import { SalesTable } from '../components/sales-table'
import { SnapTradeReference } from '../components/snaptrade-reference'
import {
  AnimatedCurrency,
  Button,
  Card,
  ChartChange,
  ScrollCueCard,
  SectionHeading,
} from '../components/ui'
import { useCsvExport } from '../hooks/use-csv-export'
import { useFinance } from '../hooks/use-finance'
import { useViewportScroll } from '../hooks/use-viewport-scroll'
import type { AccountGroup } from '../lib/account-groups'
import { accountDisplayName } from '../lib/account-names'
import { accountStartDate } from '../lib/account-start-dates'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities } from '../lib/activity'
import { openExternalUrl } from '../lib/api'
import { financeSettings } from '../lib/finance-settings'
import { formatShortDay } from '../lib/format'
import { chartPeriod } from '../lib/graph-preferences'
import { getExternalLogosEnabled } from '../lib/logos'
import type { Account, FinanceSnapshot } from '../lib/schema'
import { monthlySpendingHistory, sortTransactionsByRecency } from '../lib/spending'
import { useAccountChart } from './account-overview-chart'
import { AccountOverviewDetails, type AccountFlowSummary } from './account-overview-details'

const holdsSecurities = (type: string) => type === 'brokerage' || type === 'retirement'

// The card previews; the heading opens the account's full history in Activity.
const recentActivityLimit = 8

export function AccountOverview({
  account,
  group,
  menu,
  data,
  graphWindow,
  onGraphWindowChange,
  spendingAccountIds = [],
}: {
  /** The account shown, or the combined `all` row for All accounts and a type of account. */
  account: Account
  /** A type of account shown together, in place of one account. */
  group?: AccountGroup
  /** Balances, the page's menu, which keeps its place in every view. */
  menu: ReactNode
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
  const settings = financeSettings(committed)
  const website = settings.accountDetails?.[account.id]?.website
  const accountDisplayNames = settings.accountNames
  const accountStartDates = settings.accountStartDates
  // All accounts and a type of account each show more than one account at once.
  const isAll = !group && (account.id === 'all' || account.type === 'combined')
  const isCombined = isAll || Boolean(group)
  const memberKey = group?.accounts.map(({ id }) => id).join(',')
  const committedAccount = isCombined
    ? undefined
    : committed.accounts.find(({ id }) => id === account.id)
  // A type of account starts with its earliest account, as Rust starts its change.
  const startDate = group
    ? group.accounts
        .flatMap(({ id }) => accountStartDate(data, id, accountStartDates) ?? [])
        .toSorted()[0]
    : accountStartDate(data, isAll ? 'net-worth' : account.id, accountStartDates)
  const externalLogosEnabled = getExternalLogosEnabled()
  const displayName = group
    ? group.label
    : isAll
      ? 'All accounts'
      : accountDisplayName(account.id, account.name, accountDisplayNames)
  const relatedAccountIds = useMemo(() => {
    const own = memberKey
      ? memberKey.split(',')
      : isAll
        ? committed.accounts.filter(({ id }) => id !== 'all').map(({ id }) => id)
        : [account.id]
    // A brokerage's Plaid twin carries its cash activity.
    return new Set([
      ...own,
      ...Object.entries(committed.accountLinks ?? {}).flatMap(([plaidId, snaptradeId]) =>
        own.includes(snaptradeId) ? [plaidId] : [],
      ),
    ])
  }, [account.id, committed.accountLinks, committed.accounts, isAll, memberKey])
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
    () => data.trades.filter(({ accountId }) => isAll || relatedAccountIds.has(accountId)),
    [data.trades, isAll, relatedAccountIds],
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
        .filter(({ accountId }) => isAll || relatedAccountIds.has(accountId))
        .toSorted((left, right) => (right.value ?? -Infinity) - (left.value ?? -Infinity)),
    [data.holdings, isAll, relatedAccountIds],
  )
  const activities = useMemo(
    () =>
      buildActivities(
        { updatedAt: committed.updatedAt, transactions, trades },
        accountDisplayNames,
        externalLogosEnabled,
      ).filter(({ accountId }) => !isAll || !accountId || !spendingAccountIds.includes(accountId)),
    [
      accountDisplayNames,
      committed.updatedAt,
      spendingAccountIds,
      isAll,
      externalLogosEnabled,
      trades,
      transactions,
    ],
  )
  const monthlySpending = useMemo(
    () => monthlySpendingHistory(data.insights, account.id),
    [account.id, data.insights],
  )
  const period = chartPeriod(graphWindow)
  // What moved through the account over the period, as Rust summed it. Rust sums flows for
  // every account and each one, not for a type of account, so a group shows none.
  const transactionSummary: AccountFlowSummary = committed.insights?.flows.find(
    (flows) => flows.accountId === (isAll ? 'all' : account.id) && flows.window === period,
  ) ?? {
    // No flows from Rust for this view: every figure is unknown, never zero.
    moneyIn: null,
    moneyOut: null,
    netFlow: null,
    interest: null,
    dividends: null,
    contributions: null,
    marketChange: null,
  }
  const isInvestment = group ? holdsSecurities(group.type) : isAll || holdsSecurities(account.type)
  // A card's chart is its monthly spending; a group of cards draws their balance.
  const isCredit = !isCombined && account.type === 'credit'
  const {
    chartData,
    chartChange,
    incomplete,
    historyIsAvailable,
    chartValue,
    netDeposits,
    benchmark,
    chartEvents,
  } = useAccountChart({
    data,
    committed,
    account,
    group,
    isAll,
    isInvestment,
    graphWindow,
    startDate,
  })
  const holdingsLink = (
    <Link className="section-heading-link" to="/holdings" data-command="navigation.open./holdings">
      Holdings <ChevronRight size={14} aria-hidden="true" />
    </Link>
  )
  const activityLink = (
    <Link
      className="section-heading-link"
      to="/activities"
      search={isAll ? undefined : { account: memberKey ?? account.id }}
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
  useObjectDetailsAction(isCombined ? undefined : `account:${account.id}`)
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
        // All accounts shows every figure, so there's nothing to disclose; Cash discloses its
        // cash ahead.
        ...(isCombined && group?.type !== 'cash'
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
      [displayName, group?.type, isCombined, website],
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
            {!isCombined && account.balanceSource === 'unavailable' ? ' · value unavailable' : ''}
            {!isCombined && account.balanceSource === 'estimated' && account.balanceAsOf
              ? ` · Est. since ${formatShortDay(account.balanceAsOf)}`
              : ''}
            {group?.outsideTotal ? ' · not in total' : ''}
          </h2>
          <div className="home-balance-value">
            <AnimatedCurrency className="hero-number" value={chartValue} />
          </div>
        </div>
        {committedAccount?.id.startsWith('snaptrade:') || (!isCredit && historyIsAvailable) ? (
          <div className="chart-header-aside">
            {!isCredit && historyIsAvailable ? (
              <ChartRangeSelector value={graphWindow} onValueChange={onGraphWindowChange} />
            ) : null}
            <SnapTradeReference account={committedAccount} />
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
        <div key={group ? `group:${group.type}` : account.id} className="brokerage-chart-slide">
          <div className="brokerage-chart-content">
            {isCredit ? (
              <MonthlyBarChart data={monthlySpending} label={`${displayName} monthly spending`} />
            ) : historyIsAvailable && chartValue != null ? (
              <PerformanceChart
                data={chartData}
                netDeposits={netDeposits}
                benchmark={benchmark}
                value={chartValue}
                events={chartEvents}
                referenceIso={data.updatedAt}
                startDate={startDate}
                selectedWindow={graphWindow}
                observedOn={isCombined ? undefined : account.observedOn}
                estimatedAfter={
                  !isCombined && account.balanceSource === 'estimated'
                    ? (account.balanceAsOf ?? undefined)
                    : undefined
                }
              />
            ) : (
              <ValueHistoryEmptyState incomplete={incomplete} valueAvailable={chartValue != null} />
            )}
          </div>
        </div>
      </div>
    </Card>
  )

  // Beside the chart: the view's figures, then what an investment holds and sold, or what's
  // coming up for everything else.
  const side = (
    <>
      <AccountOverviewDetails
        account={account}
        group={group}
        isAll={isAll}
        data={data}
        committed={committed}
        summary={transactionSummary}
        period={period}
        isInvestment={isInvestment}
        isCredit={isCredit}
        transactionCount={transactions.length}
        spendingAccountIds={spendingAccountIds}
        detailsRef={detailsRef}
      />
      {isInvestment && !isAll ? (
        <>
          <Card className="account-preview-card account-holdings-preview">
            <PositionTable
              title={holdingsLink}
              positions={positions.slice(0, 5)}
              externalLogosEnabled={externalLogosEnabled}
            />
          </Card>
          {sales.length ? (
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
          ) : null}
        </>
      ) : (
        <ExpectedActivity
          data={committed}
          account={isAll ? undefined : (memberKey ?? account.id)}
          className="account-preview-card account-expected-preview"
          scrollRef={expectedScrollRef}
        />
      )}
    </>
  )

  return (
    <AccountsLayout
      snapshot={committed}
      showConnections={isAll}
      chartCard={chartCard}
      menu={menu}
      activityPreview={activityPreview}
      side={side}
    />
  )
}
