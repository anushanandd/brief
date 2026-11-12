import type { FinanceSnapshot } from './schema'

/**
 * The user's whole position in each security, one record per ticker, as Rust totaled it across
 * accounts at the values shown (a market projection carries its own). Anything about "your
 * position in X" reads these; only a view about one account reads `holdings`, which has a row
 * per account.
 */
export function consolidatedPositions(data: Pick<FinanceSnapshot, 'insights'>) {
  return data.insights?.rollup.holdings ?? []
}

export type ConsolidatedPosition = ReturnType<typeof consolidatedPositions>[number]

/** One security's position across the accounts holding it, with the positions behind it. */
export function holdingDetail(
  data: Pick<FinanceSnapshot, 'holdings' | 'insights'>,
  ticker: string,
) {
  const total = consolidatedPositions(data).find((holding) => holding.ticker === ticker)
  return {
    positions: data.holdings.filter((holding) => holding.ticker === ticker),
    name: total?.name,
    shares: total?.shares ?? null,
    vestedShares: total?.vestedShares ?? null,
    value: total?.value ?? null,
    costBasis: total?.costBasis ?? null,
    gain: total?.unrealizedGain ?? null,
    totalChange: total?.totalChangePct ?? null,
    averageBasis: total?.averageBasis ?? null,
    weight: total?.weight ?? null,
    dailyChange: total?.dailyChangePct ?? null,
    weeklyChange: total?.weeklyChangePct ?? null,
    /** Each account's part, in the order first held. */
    accounts: total?.accounts ?? [],
  }
}
