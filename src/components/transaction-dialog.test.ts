import { expect, it } from 'vitest'

import { inspectorKey } from './transaction-dialog'

it('maps the inspector keys and leaves every other key to the page', () => {
  expect(inspectorKey('j')).toBe('next')
  expect(inspectorKey('ArrowDown')).toBe('next')
  expect(inspectorKey('k')).toBe('previous')
  expect(inspectorKey('ArrowUp')).toBe('previous')
  expect(inspectorKey('c')).toBe('category')
  expect(inspectorKey('t')).toBe('tag')
  expect(inspectorKey('s')).toBe('split')
  expect(inspectorKey('f')).toBe('other')
  expect(inspectorKey('x')).toBeUndefined()
  expect(inspectorKey('Enter')).toBeUndefined()
})
