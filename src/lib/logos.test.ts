import { describe, expect, it } from 'vitest'

import {
  markColor,
  stockLogoUrl,
  stockMarkColor,
  stockMarkLabel,
  transactionLogoUrl,
  transactionMarkKind,
  transactionMarkLabel,
} from './logos'
import type { Transaction } from './schema'

const transaction = (overrides: Partial<Transaction> = {}): Transaction => ({
  id: 'transaction',
  merchant: 'Raw bank descriptor',
  category: 'Other',
  date: '2026-08-31',
  amount: -10,
  account: 'Card',
  pending: false,
  ...overrides,
})

describe('logo resolution', () => {
  it('generates local marks without requesting remote financial identifiers', () => {
    expect(stockLogoUrl('BRK.B')).toMatch(/^data:image\/svg\+xml/)
    expect(stockMarkLabel('BRK.B')).toBe('BRK')
    expect(stockMarkLabel('$CASH-USD')).toBe('$')
    expect(stockMarkColor('BRK.B')).toBe('#7399a0')
    expect(markColor('premium-savings')).not.toBe(markColor('max-rate-checking'))

    const url = transactionLogoUrl(
      transaction({
        logoUrl: 'https://plaid-merchant-logos.plaid.com/merchant.png',
        website: 'https://www.example.com/store',
        logoName: 'Example Coffee',
      }),
    )
    expect(url).toMatch(/^data:image\/svg\+xml/)
    expect(url).not.toContain('plaid')
    expect(url).not.toContain('example.com')
    expect(transactionMarkLabel(transaction({ logoName: 'Example Coffee' }))).toBe('EC')
    expect(transactionLogoUrl(transaction())).toBe(transactionLogoUrl(transaction()))
  })

  it('uses allowlisted actual logos only after external loading is enabled', () => {
    expect(stockLogoUrl('AAPL', true)).toContain('https://img.logo.dev/ticker/AAPL')
    expect(
      transactionLogoUrl(
        transaction({ logoUrl: 'https://plaid-merchant-logos.plaid.com/merchant.png' }),
        true,
      ),
    ).toBe('https://plaid-merchant-logos.plaid.com/merchant.png')
    expect(
      transactionLogoUrl(
        transaction({
          logoUrl: 'https://untrusted.example/logo.png',
          logoName: 'Example Coffee',
        }),
        true,
      ),
    ).toContain('https://img.logo.dev/name/Example%20Coffee')
  })

  it('uses meaningful local marks for non-purchase activity', () => {
    expect(
      transactionMarkKind(
        transaction({
          merchant: 'TRANSFER MONEY FROM BROKERAGE XXXXX8549 Reference Number: MCK1SOY78',
          category: 'Transfer In',
        }),
      ),
    ).toBe('transfer')
    expect(transactionMarkKind(transaction({ merchant: 'INTEREST', category: 'Income' }))).toBe(
      'interest',
    )
    expect(transactionMarkKind(transaction({ merchant: 'Dividend · VTI' }))).toBe('dividend')
    expect(transactionMarkKind(transaction({ merchant: 'Salary', category: 'Income' }))).toBe(
      'income',
    )
  })

  it('uses American Express branding for its card payments', () => {
    const bankPayment = transaction({
      merchant: 'AMEX EPAYMENT ACH PMT',
      description: 'AMEX EPAYMENT ACH PMT',
      account: 'Premium Savings -1676',
      category: 'Loan Payments',
    })
    const cardPayment = transaction({
      merchant: 'AUTOPAY PAYMENT - THANK YOU',
      account: 'Morgan Stanley Platinum Card®',
      category: 'Loan Payments',
    })

    expect(transactionMarkLabel(bankPayment)).toBe('AMEX')
    expect(transactionLogoUrl(bankPayment, true)).toContain('img.logo.dev/americanexpress.com')
    expect(transactionLogoUrl(cardPayment, true)).toContain('img.logo.dev/americanexpress.com')
    expect(
      transactionLogoUrl(
        transaction({ merchant: 'AUTOPAY PAYMENT - THANK YOU', category: 'Loan Payments' }),
        true,
      ),
    ).not.toContain('americanexpress.com')
  })
})
