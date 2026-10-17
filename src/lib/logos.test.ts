import { describe, expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { transaction as record } from '../data/fixtures/records'
import {
  institutionLogoUrl,
  brandLogoUrl,
  brandMarkLabel,
  markColor,
  stockLogoUrl,
  stockMarkColor,
  stockMarkLabel,
  transactionLogoUrl,
  transactionMarkLabel,
} from './logos'
import type { Transaction } from './schema'

const transaction = (overrides: Partial<Transaction> = {}): Transaction =>
  record({
    id: 'transaction',
    merchant: 'Raw bank descriptor',
    category: 'Other',
    date: '2026-08-31',
    amount: -10,
    account: 'Card',
    classification: classification('expense'),
    pending: false,
    ...overrides,
  })

describe('logo resolution', () => {
  it('keeps external logos off without generating transaction initials', () => {
    expect(stockLogoUrl('BRK.B')).toMatch(/^data:image\/svg\+xml/)
    expect(stockMarkLabel('BRK.B')).toBe('BRK')
    expect(stockMarkLabel('$CASH-USD')).toBe('$')
    expect(stockMarkColor('BRK.B')).toMatch(/^#[0-9a-f]{6}$/i)
    expect(markColor('premium-savings')).not.toBe(markColor('max-rate-checking'))

    const url = transactionLogoUrl(
      transaction({
        logoUrl: 'https://plaid-merchant-logos.plaid.com/merchant.png',
        website: 'https://www.example.com/store',
        logoName: 'Example Coffee',
      }),
    )
    expect(url).toBeUndefined()
    expect(transactionMarkLabel(transaction({ logoName: 'Example Coffee' }))).toBe('EC')
    expect(brandMarkLabel('Global Entry / TSA PreCheck')).toBe('GE')
    expect(brandLogoUrl('Resy', 'resy.com')).toMatch(/^data:image\/svg\+xml/)
    expect(brandLogoUrl('Resy', 'resy.com')).not.toContain('resy.com')
  })

  it('uses allowlisted actual logos only after external loading is enabled', () => {
    expect(stockLogoUrl('AAPL', true)).toContain('https://img.logo.dev/ticker/AAPL')
    expect(brandLogoUrl('Resy', 'resy.com', true)).toContain('https://img.logo.dev/resy.com')
    expect(
      transactionLogoUrl(
        transaction({ logoUrl: 'https://plaid-merchant-logos.plaid.com/merchant.png' }),
        true,
      ),
    ).toBe('https://plaid-merchant-logos.plaid.com/merchant.png')
    const merchant = (confidence: string) =>
      transaction({
        logoUrl: 'https://untrusted.example/logo.png',
        logoName: 'Example Coffee',
        counterparties: [{ name: 'Example Coffee', type: 'merchant', confidence }],
      })
    expect(transactionLogoUrl(merchant('HIGH'), true)).toContain(
      'https://img.logo.dev/name/Example%20Coffee',
    )
    expect(
      transactionLogoUrl({ ...merchant('VERY_HIGH'), website: 'https://www.example.com' }, true),
    ).toContain('https://img.logo.dev/example.com')
  })

  it('never guesses a logo for a merchant Plaid did not recognize', () => {
    for (const confidence of ['LOW', 'MEDIUM', 'UNKNOWN']) {
      const guess = transaction({
        logoName: 'Example Coffee',
        website: 'https://www.example.com',
        counterparties: [{ name: 'Example Coffee', type: 'merchant', confidence }],
      })
      expect(transactionLogoUrl(guess, true)).toBeUndefined()
    }
    // No counterparty at all, as on an imported statement row, is not recognized either.
    expect(transactionLogoUrl(transaction({ logoName: 'Example Coffee' }), true)).toBeUndefined()
    // Plaid's own logo is kept: Plaid attaches one only to a merchant it matched.
    expect(
      transactionLogoUrl(
        transaction({ logoUrl: 'https://plaid-merchant-logos.plaid.com/merchant.png' }),
        true,
      ),
    ).toBe('https://plaid-merchant-logos.plaid.com/merchant.png')
  })

  it('uses American Express branding for its card payments', () => {
    const bankPayment = transaction({
      merchant: 'AMEX EPAYMENT ACH PMT',
      description: 'AMEX EPAYMENT ACH PMT',
      account: 'Premium Savings -1676',
      category: 'Loan Payments',
      classification: classification('transfer', { mark: 'payment' }),
    })
    const cardPayment = transaction({
      merchant: 'AUTOPAY PAYMENT - THANK YOU',
      account: 'Morgan Stanley Platinum Card®',
      category: 'Loan Payments',
      classification: classification('transfer', { mark: 'payment' }),
    })

    expect(transactionMarkLabel(bankPayment)).toBe('AMEX')
    expect(transactionLogoUrl(bankPayment, true)).toContain('img.logo.dev/americanexpress.com')
    expect(transactionLogoUrl(cardPayment, true)).toContain('img.logo.dev/americanexpress.com')
    expect(
      transactionLogoUrl(
        transaction({
          merchant: 'AUTOPAY PAYMENT - THANK YOU',
          category: 'Loan Payments',
          classification: classification('transfer', { mark: 'payment' }),
        }),
        true,
      ),
    ).toBeUndefined()
  })
})

it('looks up institution logos by exact domain, only with consent', () => {
  expect(institutionLogoUrl({ institution: 'Chase', domain: 'chase.com' }, false)).toBeUndefined()
  expect(institutionLogoUrl({ institution: 'Local Credit Union' }, true)).toBeUndefined()
  // A common institution without a reported website still gets its logo.
  expect(institutionLogoUrl({ institution: 'Chase' }, true)).toMatch(/\/chase\.com\?/)
  expect(institutionLogoUrl({ institution: 'E-Trade' }, true)).toMatch(/\/etrade\.com\?/)
  expect(institutionLogoUrl({ institution: 'Chase', domain: 'chase.com' }, true)).toMatch(
    /^https:\/\/img\.logo\.dev\/chase\.com\?/,
  )
  // A brand the provider's website doesn't match is overridden by name.
  expect(
    institutionLogoUrl(
      { institution: 'E*TRADE from Morgan Stanley', domain: 'morganstanley.com' },
      true,
    ),
  ).toMatch(/^https:\/\/img\.logo\.dev\/etrade\.com\?/)
})
