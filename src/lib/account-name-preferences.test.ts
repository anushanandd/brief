import { describe, expect, it } from 'vitest'

import { accountDisplayName, parseAccountDisplayNames } from './account-name-preferences'

describe('account display names', () => {
  it('keeps only trimmed, non-empty string aliases', () => {
    expect(
      parseAccountDisplayNames(
        JSON.stringify({ brokerage: '  Long-term investing  ', card: ' ', cash: 42 }),
      ),
    ).toEqual({ brokerage: 'Long-term investing' })
    expect(parseAccountDisplayNames('{bad json')).toEqual({})
  })

  it('uses an alias without changing the provider name', () => {
    const providerName = 'Individual Brokerage'
    expect(accountDisplayName('brokerage', providerName, { brokerage: 'Investments' })).toBe(
      'Investments',
    )
    expect(accountDisplayName('other', providerName, { brokerage: 'Investments' })).toBe(
      providerName,
    )
  })
})
