import type { Transaction } from './schema'

// Logo.dev publishable keys are designed for client-side image requests.
const LOGO_DEV_PUBLISHABLE_KEY = 'pk_N6b6KnYTRcOsjU__6zKUqA'

function logoDevUrl(kind: 'name' | 'ticker' | null, value: string): string {
  const path = kind ? `${kind}/${encodeURIComponent(value)}` : encodeURIComponent(value)
  const url = new URL(`https://img.logo.dev/${path}`)
  url.searchParams.set('token', LOGO_DEV_PUBLISHABLE_KEY)
  url.searchParams.set('size', '64')
  url.searchParams.set('format', 'png')
  url.searchParams.set('fallback', '404')
  return url.toString()
}

function websiteDomain(value: string): string | null {
  try {
    const url = new URL(value.includes('://') ? value : `https://${value}`)
    return url.hostname.replace(/^www\./, '') || null
  } catch {
    return null
  }
}

export function stockLogoUrl(ticker: string): string | undefined {
  const normalized = ticker.trim()
  if (!normalized || normalized === '—') return undefined
  return logoDevUrl('ticker', normalized)
}

export function transactionLogoUrl(transaction: Transaction): string | undefined {
  if (transaction.logoUrl) return transaction.logoUrl

  const domain = transaction.website ? websiteDomain(transaction.website) : null
  if (domain) return logoDevUrl(null, domain)

  // logoName is populated only from Plaid's resolved merchant data. The raw bank descriptor is
  // deliberately excluded so private transaction text is never sent to the logo provider.
  if (transaction.logoName) return logoDevUrl('name', transaction.logoName)
  return undefined
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
