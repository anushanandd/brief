import { Link } from '@tanstack/react-router'

import { useLiveFinance } from '../hooks/use-live-finance'
import { type ChartPeriod } from '../lib/graph-preferences'
import { useInvestmentRange } from '../lib/investment-range'
import type { FinanceSnapshot, PortfolioToday } from '../lib/schema'
import { PageError, PageLoading } from './data-state'
import { HoldingChart } from './holding-chart'
import { HoldingAccounts, HoldingActivity, PositionCard } from './holding-details'
import { HoldingNews } from './holding-news'
import { HoldingsTable } from './holdings-table'
import { InvestmentSummaryCard, useInvestmentSummary } from './investment-summary-card'
import { InvestmentsPortfolio } from './investment-value'
import { InvestmentsRail, RETIREMENT } from './investments-rail'
import {
  InvestmentsAttention,
  InvestmentsComingUp,
  InvestmentsPerformance,
  PeriodCard,
  periodStatement,
  useInvestmentRangeShortcuts,
  usePortfolioToday,
} from './investments-summary'
import { PortfolioExposure } from './portfolio-exposure'
import { RetirementView } from './retirement'
import { Card, EmptyState } from './ui'
import { MarketStatus, WorkspaceHeader } from './workspace-header'

type Integrations = ReturnType<typeof useLiveFinance>['integrationStatus']

/**
 * The portfolio: its value and chart with the summary under them, the period's figures and
 * what's ahead beside them, then every holding, and allocation beside performance.
 */
function Overview({
  data,
  today,
  value,
  range,
  integrations,
}: {
  data: FinanceSnapshot
  today?: PortfolioToday
  value: number | null
  range: ChartPeriod
  integrations: Integrations
}) {
  const span = periodStatement(data, range)
  // The table's news column reads the summary the card beside it loads.
  const { summary } = useInvestmentSummary(data, range)
  return (
    <>
      <div className="investments-hero">
        <div className="investments-main">
          <InvestmentsPortfolio data={data} value={value} range={range} />
          <InvestmentSummaryCard data={data} range={range} />
        </div>
        <div className="investments-side">
          <PeriodCard data={data} span={span} range={range} />
          <InvestmentsComingUp
            data={data}
            earningsConnected={integrations.alphaVantage}
            summary={summary.data}
          />
          <InvestmentsAttention data={data} />
        </div>
      </div>
      <HoldingsTable data={data} range={range} today={today} summary={summary.data} />
      <div className="investments-pair">
        <PortfolioExposure data={data} />
        <InvestmentsPerformance data={data} range={range} />
      </div>
    </>
  )
}

/**
 * One holding: its chart with the summary under it, the position beside them, then its news
 * beside where it's held and what you did.
 */
function Holding({
  data,
  ticker,
  range,
  integrations,
}: {
  data: FinanceSnapshot
  ticker: string
  range: ChartPeriod
  integrations: Integrations
}) {
  // Largest first, the order the strip and the chart's next and previous follow.
  const order = (data.insights?.securities ?? []).map((security) => security.ticker)
  const holdings = data.holdings.toSorted(
    (left, right) => order.indexOf(left.ticker) - order.indexOf(right.ticker),
  )
  if (!holdings.some((holding) => holding.ticker === ticker))
    return (
      <Card>
        <EmptyState title={`${ticker} isn’t held`}>
          <Link to="/holdings" data-command="navigation.open./holdings">
            Back to all investments
          </Link>
        </EmptyState>
      </Card>
    )
  return (
    <>
      <div className="investments-hero">
        <div className="investments-main">
          <HoldingChart holdings={holdings} ticker={ticker} range={range} />
          <InvestmentSummaryCard data={data} range={range} ticker={ticker} />
        </div>
        <div className="investments-side">
          <PositionCard data={data} ticker={ticker} earningsConnected={integrations.alphaVantage} />
          <HoldingAccounts data={data} ticker={ticker} />
        </div>
      </div>
      <div className="holding-columns">
        <HoldingNews
          ticker={ticker}
          connected={integrations.alpaca}
          secConnected={integrations.sec}
        />
        <HoldingActivity data={data} ticker={ticker} />
      </div>
    </>
  )
}

/**
 * Investments: the whole portfolio, or one holding, chosen from the strip of cards that heads
 * the page. One period, chosen in the main chart card, decides every figure. Selection is the
 * URL, so each holding has its own address.
 */
export function InvestmentsWorkspace({
  ticker,
  retirement = false,
}: {
  ticker?: string
  retirement?: boolean
}) {
  const query = useLiveFinance()
  const today = usePortfolioToday(query.data?.revision, query.integrationStatus.alpaca)
  const range = useInvestmentRange()
  useInvestmentRangeShortcuts()

  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />
  const data = query.data
  const value =
    data.brokeragePerformance.find(({ accountId }) => accountId === 'total')?.currentValue ??
    data.insights?.allocation?.total ??
    null
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
      <InvestmentsRail
        data={data}
        range={range}
        selected={retirement ? RETIREMENT : ticker}
        value={value}
      />
      <div
        className="investments-pane"
        aria-label={ticker ? `${ticker} detail` : retirement ? 'Retirement' : 'All investments'}
      >
        {retirement ? (
          <RetirementView data={data} />
        ) : ticker ? (
          <Holding
            data={data}
            ticker={ticker}
            range={range}
            integrations={query.integrationStatus}
          />
        ) : (
          <Overview
            data={data}
            today={today.data}
            value={value}
            range={range}
            integrations={query.integrationStatus}
          />
        )}
      </div>
    </div>
  )
}
