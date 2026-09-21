import type { FinanceSnapshot } from './schema'

export function holdingDetail(holdings: FinanceSnapshot['holdings'], ticker: string) {
  const positions = holdings.filter((holding) => holding.ticker === ticker)
  const sum = (field: 'shares' | 'value' | 'costBasis') =>
    positions.length && positions.every((position) => position[field] != null)
      ? positions.reduce((total, position) => total + position[field]!, 0)
      : null
  const shares = sum('shares')
  const value = sum('value')
  const basis = sum('costBasis')
  const knownInvestments = holdings.reduce(
    (total, holding) => total + Math.max(0, holding.value ?? 0),
    0,
  )
  const change = (field: 'dailyChangePct' | 'weeklyChangePct') => {
    const first = positions[0]?.[field] ?? null
    return positions.every((position) => position[field] === first) ? first : null
  }
  return {
    positions,
    shares,
    value,
    basis,
    totalChange:
      value != null && basis != null && basis > 0 ? ((value - basis) / basis) * 100 : null,
    averageBasis: shares != null && shares > 0 && basis != null ? basis / shares : null,
    weight:
      value != null && value >= 0 && knownInvestments > 0 ? (value / knownInvestments) * 100 : null,
    dailyChange: change('dailyChangePct'),
    weeklyChange: change('weeklyChangePct'),
  }
}
