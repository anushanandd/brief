import { type ChartPeriod } from '../lib/graph-preferences'
import { holdingDetail } from '../lib/holding-detail'
import type { FinanceSnapshot } from '../lib/schema'
import { AccountMark } from './account-mark'
import { useInvestmentSummary } from './investment-summary-card'
import { periodStatement } from './investments-summary'
import { SecurityMark } from './security-mark'
import { SelectorStrip } from './selector-strip'

/** The strip card that opens Retirement, which no ticker can collide with. */
export const RETIREMENT = 'retirement'

/**
 * The page's selector, as one strip of cards: the portfolio, retirement, then each holding
 * largest first, each with its worth and what it earned over the page's period: the
 * portfolio's investment gain, and each holding's dollars from the period's attribution.
 * Retirement's worth is its accounts' value today; contributions aren't a period's change.
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
  const retirementAccounts = data.accounts.filter(({ type }) => type === 'retirement')
  const retirementValue = retirementAccounts.every((account) => account.value != null)
    ? retirementAccounts.reduce((total, account) => total + (account.value ?? 0), 0)
    : null
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
        {
          id: RETIREMENT,
          label: 'Retirement',
          mark: <AccountMark type="retirement" />,
          link: { to: '/holdings/retirement' },
          current: selected === RETIREMENT,
          value: retirementValue,
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
