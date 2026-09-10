import { describe, expect, it } from 'vitest'

import {
  defaultHiddenPlatinumBenefitIds,
  parseHiddenPlatinumBenefitIds,
} from './spending-preferences'

describe('spending preferences', () => {
  it('defaults to the optional Platinum benefits and preserves an explicit empty selection', () => {
    expect(parseHiddenPlatinumBenefitIds(null)).toEqual(defaultHiddenPlatinumBenefitIds)
    expect(parseHiddenPlatinumBenefitIds('')).toEqual([])
    expect(parseHiddenPlatinumBenefitIds('clear,oura')).toEqual(['clear', 'oura'])
  })
})
