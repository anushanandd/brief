import { expect, it } from 'vitest'

import { briefMerchant } from './insights'

it('shortens repeated merchant names and company suffixes', () => {
  expect(briefMerchant('Example AI Sol EXAMPLE')).toBe('Example AI')
})
