import { describe, expect, it } from 'vitest'

import { buildNetWorthAllocation } from './allocation'

describe('buildNetWorthAllocation', () => {
  it('combines investment accounts into one brokerage slice', () => {
    const allocation = buildNetWorthAllocation({
      accounts: [
        { id: 'all', name: 'All', institution: 'Brief', type: 'combined', value: 170 },
        { id: 'broker', name: 'Brokerage', institution: 'Etrade', type: 'brokerage', value: 100 },
        { id: 'ira', name: 'IRA', institution: 'Schwab', type: 'retirement', value: 50 },
        { id: 'cash', name: 'Checking', institution: 'Bank', type: 'cash', value: 30 },
        { id: 'debt', name: 'Card', institution: 'Bank', type: 'credit', value: -10 },
      ],
    })

    expect(allocation.map(({ name, value }) => ({ name, value }))).toEqual([
      { name: 'Brokerage', value: 150 },
      { name: 'Checking', value: 30 },
    ])
    expect(allocation.reduce((sum, item) => sum + item.percent, 0)).toBeCloseTo(100)
  })
})
