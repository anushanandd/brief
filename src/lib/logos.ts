import type { Transaction } from './schema'

const externalLogosKey = 'brief.externalLogosEnabled'
const logoDevPublishableKey = 'pk_N6b6KnYTRcOsjU__6zKUqA'
const allowedProviderLogoHosts = new Set(['plaid-merchant-logos.plaid.com'])

const markPalette = [
  ['#6f92b5', '#08090a'],
  ['#71a18b', '#08090a'],
  ['#bd9668', '#08090a'],
  ['#9584b0', '#08090a'],
  ['#ae7884', '#08090a'],
  ['#7399a0', '#08090a'],
  ['#8e9e67', '#08090a'],
] as const

const ignoredMerchantWords = new Set([
  'and',
  'co',
  'company',
  'corp',
  'corporation',
  'inc',
  'llc',
  'online',
  'payment',
  'purchase',
  'the',
])

function markPaletteEntry(label: string) {
  const paletteIndex = label
    .split('')
    .reduce((hash, character) => (hash * 31 + character.charCodeAt(0)) % markPalette.length, 0)
  return markPalette[paletteIndex]
}

export function markColor(label: string): string {
  return markPaletteEntry(label)[0]
}

function markUrl(label: string): string {
  const [background, foreground] = markPaletteEntry(label)
  const fontSize = label.length >= 4 ? 20 : label.length === 3 ? 23 : 28
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><rect width="64" height="64" rx="16" fill="${background}"/><text x="32" y="33" fill="${foreground}" font-family="-apple-system,BlinkMacSystemFont,Arial,sans-serif" font-size="${fontSize}" font-weight="700" text-anchor="middle" dominant-baseline="middle">${label}</text></svg>`

  return `data:image/svg+xml;charset=utf-8,${encodeURIComponent(svg)}`
}

function logoDevUrl(kind: 'name' | 'ticker' | null, value: string): string {
  const path = kind ? `${kind}/${encodeURIComponent(value)}` : encodeURIComponent(value)
  const url = new URL(`https://img.logo.dev/${path}`)
  url.searchParams.set('token', logoDevPublishableKey)
  url.searchParams.set('size', '64')
  url.searchParams.set('format', 'png')
  url.searchParams.set('fallback', '404')
  return url.toString()
}

function websiteDomain(value: string): string | undefined {
  try {
    const url = new URL(value.includes('://') ? value : `https://${value}`)
    return url.hostname.replace(/^www\./, '') || undefined
  } catch {
    return undefined
  }
}

function allowedProviderLogo(value?: string): string | undefined {
  if (!value) return undefined
  try {
    const url = new URL(value)
    return url.protocol === 'https:' && allowedProviderLogoHosts.has(url.hostname)
      ? url.toString()
      : undefined
  } catch {
    return undefined
  }
}

export function getExternalLogosEnabled(): boolean {
  return typeof window !== 'undefined' && window.localStorage.getItem(externalLogosKey) === 'true'
}

export function saveExternalLogosEnabled(enabled: boolean) {
  window.localStorage.setItem(externalLogosKey, String(enabled))
}

export function stockMarkLabel(ticker: string): string {
  const normalized = ticker.trim().toUpperCase()
  if (/^\$?CASH(?:[-_:][A-Z]{3})?$/.test(normalized)) return '$'
  return (
    normalized
      .split(/[.:-]/)[0]
      ?.replace(/[^A-Z0-9]/g, '')
      .slice(0, 4) || '•'
  ).toUpperCase()
}

function isAmericanExpressPayment(transaction: Transaction): boolean {
  const text = `${transaction.merchant} ${transaction.description ?? ''} ${transaction.account}`
  return (
    transactionMarkKind(transaction) === 'payment' &&
    /\bamex\b|american express|morgan stanley platinum card/i.test(text)
  )
}

export function transactionMarkLabel(transaction: Transaction): string {
  if (isAmericanExpressPayment(transaction)) return 'AMEX'
  const source = transaction.logoName?.trim() || transaction.merchant.trim()
  const words = source
    .normalize('NFKD')
    .replace(/[^a-zA-Z0-9]+/g, ' ')
    .trim()
    .split(/\s+/)
    .filter((word) => word && !ignoredMerchantWords.has(word.toLocaleLowerCase()))
  const label =
    words.length > 1
      ? words
          .slice(0, 2)
          .map((word) => word[0])
          .join('')
      : words[0]?.slice(0, 2)
  return label?.toUpperCase() || '•'
}

export function stockMarkColor(ticker: string): string {
  return markColor(stockMarkLabel(ticker))
}

export function stockLogoUrl(ticker: string, external = false): string {
  const normalized = ticker.trim()
  return external && normalized && normalized !== '—'
    ? logoDevUrl('ticker', normalized)
    : markUrl(stockMarkLabel(ticker))
}

export function transactionLogoUrl(transaction: Transaction, external = false): string {
  if (!external) return markUrl(transactionMarkLabel(transaction))
  const providerLogo = allowedProviderLogo(transaction.logoUrl)
  if (providerLogo) return providerLogo
  if (isAmericanExpressPayment(transaction)) return logoDevUrl(null, 'americanexpress.com')
  const domain = transaction.website ? websiteDomain(transaction.website) : undefined
  if (domain) return logoDevUrl(null, domain)
  return transaction.logoName
    ? logoDevUrl('name', transaction.logoName)
    : markUrl(transactionMarkLabel(transaction))
}

export type TransactionMarkKind =
  | 'transfer'
  | 'interest'
  | 'dividend'
  | 'income'
  | 'refund'
  | 'fee'
  | 'payment'
  | 'cash'
  | 'initial'

/** Chooses a private, local fallback mark for activity that does not have a merchant logo. */
export function transactionMarkKind(transaction: Transaction): TransactionMarkKind {
  const category = transaction.category.toLocaleLowerCase()
  const merchant = transaction.merchant.toLocaleLowerCase()
  const text = `${category} ${merchant}`

  if (/interest|\bapy\b/.test(text)) return 'interest'
  if (/dividend|capital gain|investment distribution/.test(text)) return 'dividend'
  if (/refund|reimbursement|cash ?back|reversal|credit adjustment/.test(text)) return 'refund'
  if (/\bfee\b|overdraft|service charge/.test(text)) return 'fee'
  if (/cash withdrawal|\batm\b/.test(text)) return 'cash'
  if (/credit card payment|loan payment|autopay|bill payment/.test(text)) return 'payment'
  if (/transfer|\bwire\b|\bach\b/.test(text)) return 'transfer'
  if (/income|salary|payroll|direct deposit/.test(text)) return 'income'
  return 'initial'
}
