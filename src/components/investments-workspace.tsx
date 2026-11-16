import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'

import { useAsOf } from '../hooks/use-as-of'
import { useLiveFinance } from '../hooks/use-live-finance'
import { type ChartPeriod } from '../lib/graph-preferences'
import { consolidatedPositions } from '../lib/holding-detail'
import { useInvestmentRange } from '../lib/investment-range'
import type { FinanceSnapshot } from '../lib/schema'
import { PageError, PageLoading } from './data-state'
import { EarningsReport } from './earnings-report'
import { HoldingChart } from './holding-chart'
import { HoldingAccounts, HoldingActivity, isFundKind, PositionCard } from './holding-details'
import { HoldingNews } from './holding-news'
import { HoldingsTable, RETIREMENT } from './holdings-table'
import { InvestmentSummaryCard, useInvestmentSummary } from './investment-summary-card'
import { InvestmentsPortfolio } from './investment-value'
import {
  InvestmentsAttention,
  InvestmentsComingUp,
  InvestmentsPerformance,
  PeriodCard,
  periodStatement,
  useInvestmentRangeShortcuts,
} from './investments-summary'
import { PortfolioExposure } from './portfolio-exposure'
import { RetirementView } from './retirement'
import { Card, EmptyState } from './ui'
import { MarketStatus, WorkspaceHeader } from './workspace-header'

type Integrations = ReturnType<typeof useLiveFinance>['integrationStatus']

/**
 * The portfolio: its value and chart beside the period's figures, the menu beside the summary,
 * then what's ahead beside what needs you, and allocation beside performance.
 */
function Overview({
  data,
  today,
  menu,
  value,
  range,
  integrations,
}: {
  data: FinanceSnapshot
  /** The snapshot as it stands today, for what's ahead and the summary. */
  today: FinanceSnapshot
  menu: ReactNode
  value: number | null
  range: ChartPeriod
  integrations: Integrations
}) {
  const span = periodStatement(data, range)
  // What's ahead reads the summary the card loads, once it is current; an older one being
  // brought up to date is shown only in the card, marked. Both stay on today while the page is
  // scrolled back.
  const { current } = useInvestmentSummary(today, range)
  return (
    <>
      <div className="investments-hero">
        <InvestmentsPortfolio data={data} value={value} range={range} />
        <PeriodCard data={data} span={span} range={range} />
      </div>
      <div className="investments-split">
        {menu}
        <InvestmentSummaryCard data={today} range={range} />
      </div>
      <div className="investments-pair">
        <InvestmentsComingUp
          data={data}
          earningsConnected={integrations.alphaVantage}
          summary={current}
        />
        <InvestmentsAttention data={data} />
      </div>
      <div className="investments-pair">
        <PortfolioExposure data={data} />
        <InvestmentsPerformance data={data} range={range} />
      </div>
    </>
  )
}

/**
 * One holding: its chart beside the position, the menu beside its summary, then its news beside
 * where it's held, its latest earnings, and what you did. A fund reports no earnings, so it has
 * no earnings card.
 */
function Holding({
  data,
  today,
  menu,
  ticker,
  range,
  integrations,
}: {
  data: FinanceSnapshot
  /** The snapshot as it stands today, for the summary and the latest earnings. */
  today: FinanceSnapshot
  menu: ReactNode
  ticker: string
  range: ChartPeriod
  integrations: Integrations
}) {
  // Each security once, largest first: the order the strip and the chart's next and previous
  // follow.
  const order = (data.insights?.securities ?? []).map((security) => security.ticker)
  const holdings = consolidatedPositions(data).toSorted(
    (left, right) => order.indexOf(left.ticker) - order.indexOf(right.ticker),
  )
  if (!data.holdings.some((holding) => holding.ticker === ticker))
    return (
      <div className="investments-split">
        {menu}
        <Card>
          <EmptyState title={`${ticker} isn’t held`}>
            <Link to="/holdings" data-command="navigation.open./holdings">
              Back to all investments
            </Link>
          </EmptyState>
        </Card>
      </div>
    )
  const fund = data.holdings.some(
    (holding) => holding.ticker === ticker && isFundKind(holding.instrumentKind),
  )
  return (
    <>
      <div className="investments-hero">
        <HoldingChart holdings={holdings} ticker={ticker} range={range} />
        <PositionCard data={data} ticker={ticker} earningsConnected={integrations.alphaVantage} />
      </div>
      <div className="investments-split">
        {menu}
        <InvestmentSummaryCard data={today} range={range} ticker={ticker} />
      </div>
      <div className="holding-layout">
        <HoldingNews ticker={ticker} connected={integrations.alpaca} />
        <div className="holding-side">
          <HoldingAccounts data={data} ticker={ticker} />
          {fund ? null : <EarningsReport data={today} ticker={ticker} range={range} />}
          <HoldingActivity data={data} ticker={ticker} />
        </div>
      </div>
    </>
  )
}

/**
 * Investments: the whole portfolio, retirement, or one holding, chosen from the Holdings menu
 * each view places beside its summary. One period, chosen in the main chart card, decides every
 * figure. Selection is the URL, so each holding has its own address.
 */
export function InvestmentsWorkspace({
  ticker,
  retirement = false,
}: {
  ticker?: string
  retirement?: boolean
}) {
  const query = useLiveFinance()
  const range = useInvestmentRange()
  useInvestmentRangeShortcuts()
  // Scrolled back in time, the period's figures are the ones Rust measured to the end day.
  const past = useAsOf(query.data)

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data || !past) return <PageError />
  const data = query.data
  const value =
    data.brokeragePerformance.find(({ accountId }) => accountId === 'total')?.currentValue ??
    data.insights?.allocation?.total ??
    null
  const menu = (
    <HoldingsTable data={data} selected={retirement ? RETIREMENT : ticker} value={value} />
  )
  return (
    <div className="page holdings-page investments-page">
      <WorkspaceHeader
        title="Investments"
        status={<MarketStatus />}
        breadcrumbs={
          ticker || retirement
            ? [
                { label: 'Investments', to: '/holdings', search: {} },
                { label: ticker ?? 'Retirement', to: '/holdings', search: {} },
              ]
            : undefined
        }
      />
      <div
        className="investments-pane"
        aria-label={ticker ? `${ticker} detail` : retirement ? 'Retirement' : 'All investments'}
      >
        {retirement ? (
          <RetirementView data={data} menu={menu} />
        ) : ticker ? (
          <Holding
            data={past}
            today={data}
            menu={menu}
            ticker={ticker}
            range={range}
            integrations={query.integrationStatus}
          />
        ) : (
          <Overview
            data={past}
            today={data}
            menu={menu}
            value={value}
            range={range}
            integrations={query.integrationStatus}
          />
        )}
      </div>
    </div>
  )
}
