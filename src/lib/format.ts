const currencyFormatter = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  maximumFractionDigits: 2,
})
const compactCurrencyFormatter = new Intl.NumberFormat('en-US', {
  style: 'currency',
  currency: 'USD',
  notation: 'compact',
  maximumFractionDigits: 1,
})
const updatedAtFormatter = new Intl.DateTimeFormat('en-US', {
  hour: 'numeric',
  minute: '2-digit',
})
export const formatCurrency = (value: number | null | undefined) =>
  value == null ? '—' : currencyFormatter.format(value)

export const formatCompactCurrency = (value: number) => compactCurrencyFormatter.format(value)

export const formatPercent = (value: number | null | undefined) =>
  value == null ? '—' : `${value >= 0 ? '+' : ''}${value.toFixed(2)}%`

export const formatUpdatedAt = (iso: string) => updatedAtFormatter.format(new Date(iso))

export const formatSecurityName = (name: string) => name.replace(/\s+class\b.*$/i, '').trim()
