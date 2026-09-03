import type { FinanceSnapshot } from './schema'

const accountColors = ['#477d75', '#a46d46', '#8b8f8c', '#c1b9ad']

export function buildNetWorthAllocation({ accounts }: Pick<FinanceSnapshot, 'accounts'>) {
  const brokerageValue = accounts
    .slice(1)
    .filter((account) => account.type === 'brokerage' || account.type === 'retirement')
    .reduce((sum, account) => sum + Math.max(account.value, 0), 0)
  const slices = brokerageValue
    ? [{ name: 'Brokerage', value: brokerageValue, color: accountColors[0] }]
    : []

  for (const account of accounts.slice(1)) {
    // ponytail: pie slices show positive assets; add a liability ring if debt composition matters.
    const isInvestment = account.type === 'brokerage' || account.type === 'retirement'
    if (account.value <= 0 || isInvestment) continue
    slices.push({
      name: account.name,
      value: account.value,
      color: accountColors[slices.length % accountColors.length],
    })
  }

  const total = slices.reduce((sum, item) => sum + item.value, 0)
  return slices
    .toSorted((left, right) => right.value - left.value)
    .map((item) => ({ ...item, percent: total ? (item.value / total) * 100 : 0 }))
}
