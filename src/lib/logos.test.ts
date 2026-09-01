import { describe, expect, it } from 'vitest'

import { stockLogoUrl, transactionLogoUrl, transactionMarkKind } from './logos'
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
  it('resolves stock tickers through the Logo.dev ticker endpoint', () => {
    expect(stockLogoUrl('BRK.B')).toContain('/ticker/BRK.B?')
  })

  it('prefers Plaid logos before website and merchant fallbacks', () => {
    expect(
      transactionLogoUrl(
        transaction({
          logoUrl: 'https://plaid-merchant-logos.plaid.com/merchant.png',
          website: 'example.com',
          logoName: 'Example',
        }),
      ),
    ).toBe('https://plaid-merchant-logos.plaid.com/merchant.png')
  })

  it('uses verified domains but never sends raw descriptors', () => {
    expect(transactionLogoUrl(transaction({ website: 'https://www.example.com/store' }))).toContain(
      'https://img.logo.dev/example.com?',
    )
    expect(transactionLogoUrl(transaction())).toBeUndefined()
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
})
