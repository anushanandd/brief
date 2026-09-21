import { describe, expect, it } from 'vitest'

import {
  defaultHiddenPlatinumBenefitIds,
  parseHiddenPlatinumBenefitIds,
} from './spending-preferences'

describe('spending preferences', () => {
  it('shows all benefits by default and preserves explicit visibility choices', () => {
    expect(defaultHiddenPlatinumBenefitIds).toEqual([])
    expect(parseHiddenPlatinumBenefitIds(null)).toEqual([])
    expect(parseHiddenPlatinumBenefitIds('')).toEqual([])
    expect(parseHiddenPlatinumBenefitIds('clear,oura')).toEqual(['clear', 'oura'])
  })
})
