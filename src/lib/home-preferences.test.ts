import { expect, it } from 'vitest'

import { parseAccountGroupsMode } from './home-preferences'

it('keeps Overview account categories collapsed unless expanded was saved', () => {
  expect(parseAccountGroupsMode('expanded')).toBe('expanded')
  expect(parseAccountGroupsMode('collapsed')).toBe('collapsed')
  expect(parseAccountGroupsMode(null)).toBe('collapsed')
  expect(parseAccountGroupsMode('open')).toBe('collapsed')
})
