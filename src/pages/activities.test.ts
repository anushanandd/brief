import { expect, it } from 'vitest'

import { searchShortcutAction } from '../hooks/use-search-shortcuts'
import { accountActivityPreviewLimit } from './account-detail'
import { activityPageSize } from './activities'

it('fits only complete activity rows in the available space', () => {
  expect(activityPageSize(61)).toBe(1)
  expect(activityPageSize(124)).toBe(2)
  expect(activityPageSize(185)).toBe(2)
  expect(accountActivityPreviewLimit(4, 0)).toBe(6)
  expect(accountActivityPreviewLimit(2, 1)).toBe(4)
  expect(accountActivityPreviewLimit(3, 3)).toBe(8)
})

it('focuses, clears, and blurs ledger search without stealing input', () => {
  const slash = { key: '/', metaKey: false, ctrlKey: false, altKey: false }
  const escape = { ...slash, key: 'Escape' }
  expect(searchShortcutAction(slash, false, false, false)).toBe('focus')
  expect(searchShortcutAction(slash, true, false, false)).toBeUndefined()
  expect(searchShortcutAction({ ...slash, metaKey: true }, false, false, false)).toBeUndefined()
  expect(searchShortcutAction(escape, true, true, true)).toBe('clear')
  expect(searchShortcutAction(escape, true, true, false)).toBe('blur')
})
