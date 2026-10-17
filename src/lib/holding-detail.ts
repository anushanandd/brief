import type { FinanceSnapshot } from './schema'

/**
 * One security's position across the accounts holding it, as Rust totaled it at the values
 * shown, with the positions behind it.
 */
export function holdingDetail(
  data: Pick<FinanceSnapshot, 'holdings' | 'insights'>,
  ticker: string,
) {
  const total = data.insights?.rollup.holdings.find((holding) => holding.ticker === ticker)
  return {
    positions: data.holdings.filter((holding) => holding.ticker === ticker),
    shares: total?.shares ?? null,
    vestedShares: total?.vestedShares ?? null,
    value: total?.value ?? null,
    basis: total?.basis ?? null,
    totalChange: total?.totalChangePct ?? null,
    averageBasis: total?.averageBasis ?? null,
    weight: total?.weight ?? null,
    dailyChange: total?.dailyChangePct ?? null,
    weeklyChange: total?.weeklyChangePct ?? null,
    /** Each account's part, in the order first held. */
    accounts: total?.accounts ?? [],
  }
}
