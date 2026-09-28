import { expect, it } from 'vitest'

import { viewportScrollHeight } from './use-viewport-scroll'

it('keeps a list within the visible window while leaving a usable scroll area', () => {
  expect(viewportScrollHeight(300, 900, 36)).toBe(564)
  expect(viewportScrollHeight(820, 900, 36)).toBe(120)
})
