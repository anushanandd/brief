import { type ChartPeriod } from '../lib/graph-preferences'
import { holdingDetail } from '../lib/holding-detail'
import type { FinanceSnapshot } from '../lib/schema'
import { AccountMark } from './account-mark'
import { useInvestmentSummary } from './investment-summary-card'
import { periodStatement } from './investments-summary'
import { SecurityMark } from './security-mark'
import { SelectorStrip } from './selector-strip'

/**
 * The page's selector, as one strip of cards: the portfolio, then each holding largest first,
 * each with its worth and what it earned over the page's period: the portfolio's investment
 * gain, and each holding's dollars from the period's attribution.
 */
export function InvestmentsRail({
  data,
  range,
  selected,
  value,
}: {
  data: FinanceSnapshot
  range: ChartPeriod
  selected?: string
  value: number | null
}) {
  const span = periodStatement(data, range)
  const { summary } = useInvestmentSummary(data, range)
  const contributions = summary.data?.report?.attribution?.contributions ?? []
  const securities = data.insights?.securities ?? []
  // Holdings without insights (no USD value) still get a card, after the valued ones.
  const tickers = [
    ...new Set([
      ...securities.map((security) => security.ticker),
      ...data.holdings.map((holding) => holding.ticker),
    ]),
  ]
  return (
    <SelectorStrip
      label="Holdings"
      items={[
        {
          id: 'portfolio',
          label: 'Portfolio',
          mark: <AccountMark type="brokerage" />,
          link: { to: '/holdings' },
          current: !selected,
          value,
          change: span?.investmentGain ?? null,
        },
        ...tickers.map((ticker) => {
          const security = securities.find((item) => item.ticker === ticker)
          return {
            id: ticker,
            label: ticker,
            mark: <SecurityMark ticker={ticker} name={security?.name} />,
            link: { to: '/holdings/$ticker' as const, params: { ticker } },
            current: selected === ticker,
            value: security?.value ?? holdingDetail(data.holdings, ticker).value ?? null,
            change: contributions.find((item) => item.key === ticker)?.dollars ?? null,
          }
        }),
      ]}
    />
  )
}
