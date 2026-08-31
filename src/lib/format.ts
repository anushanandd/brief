export const formatCurrency = (value: number, options: Intl.NumberFormatOptions = {}) =>
  new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    maximumFractionDigits: 2,
    ...options,
  }).format(value)

export const formatCompactCurrency = (value: number) =>
  new Intl.NumberFormat('en-US', {
    style: 'currency',
    currency: 'USD',
    notation: 'compact',
    maximumFractionDigits: 1,
  }).format(value)

export const formatPercent = (value: number) => `${value >= 0 ? '+' : ''}${value.toFixed(2)}%`

export const formatUpdatedAt = (iso: string) =>
  new Intl.DateTimeFormat('en-US', {
    hour: 'numeric',
    minute: '2-digit',
  }).format(new Date(iso))
