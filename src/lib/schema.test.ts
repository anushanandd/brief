import { describe, expect, it } from 'vitest'

import seed from '../data/seed.json'
import { financeSnapshotSchema } from './schema'

describe('finance snapshot migrations', () => {
  it('derives cost basis for snapshots saved before the field existed', () => {
    const legacy = {
      ...seed,
      holdings: seed.holdings.map(({ costBasis: _costBasis, ...holding }) => holding),
    }

    const snapshot = financeSnapshotSchema.parse(legacy)

    expect(snapshot.holdings[0]?.costBasis).toBeCloseTo(63602.67, 2)
  })
})
