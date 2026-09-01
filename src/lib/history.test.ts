import { describe, expect, it } from 'vitest'

import { buildNetWorthHistory } from './history'
import type { Transaction } from './schema'

const transaction = (overrides: Partial<Transaction>): Transaction => ({
  id: 'transaction',
  merchant: 'Activity',
  category: 'Other',
  date: '2026-08-30',
  amount: 0,
  account: 'Account',
  pending: false,
  ...overrides,
})

describe('net worth history', () => {
  it('reconstructs posted cash flow without counting internal transfers twice', () => {
    const history = buildNetWorthHistory(
      [],
      [
        transaction({ id: 'deposit', amount: 150 }),
        transaction({ id: 'withdrawal', amount: -150 }),
        transaction({ id: 'purchase', amount: -20 }),
      ],
      1_000,
      new Date('2026-08-31T12:00:00Z'),
      3,
    )

    expect(history).toEqual([
      { date: '2026-08-29', value: 1020 },
      { date: '2026-08-30', value: 1000 },
      { date: '2026-08-31', value: 1000 },
    ])
  })

  it('keeps recorded history once a real trend exists', () => {
    expect(
      buildNetWorthHistory(
        [
          { date: '2026-08-29', value: 900 },
          { date: '2026-08-30', value: 950 },
        ],
        [],
        1_000,
        new Date('2026-08-31T12:00:00Z'),
      ),
    ).toEqual([
      { date: '2026-08-29', value: 900 },
      { date: '2026-08-30', value: 950 },
      { date: '2026-08-31', value: 1000 },
    ])
  })

  it('preserves display-ready monthly history used by the browser demo', () => {
    const history = [
      { date: 'Jul', value: 900 },
      { date: 'Aug', value: 1_000 },
    ]

    expect(buildNetWorthHistory(history, [], 1_000, new Date('2026-08-31T12:00:00Z'))).toEqual(
      history,
    )
  })
})
