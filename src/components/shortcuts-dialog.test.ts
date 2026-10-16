import { expect, it, vi } from 'vitest'

import type { AppAction } from '../lib/action-system'
import { shortcutSections } from './shortcuts-dialog'

const action = (overrides: Partial<AppAction> & Pick<AppAction, 'id' | 'title'>): AppAction => ({
  scope: 'app',
  run: vi.fn(),
  ...overrides,
})

it('groups bound actions like the app and keeps at most two bindings each', () => {
  const sections = shortcutSections([
    action({
      id: 'palette.open',
      title: 'Open command palette',
      bindings: [{ sequence: ['Meta+k'], label: '⌘K' }],
    }),
    action({
      id: 'navigation.open./',
      title: 'Open Home',
      group: 'Navigate',
      bindings: [
        { sequence: ['Meta+1'], label: '⌘1' },
        { sequence: ['g', 'h'], label: 'g h' },
        { sequence: ['g', '1'], label: 'g 1' },
      ],
    }),
    action({
      id: 'spending.week',
      title: 'Show Week',
      scope: 'page',
      group: 'View',
      bindings: [{ sequence: ['w'], label: 'W' }],
    }),
    action({
      id: 'motion.next:List',
      title: 'Next item in List',
      scope: 'region',
      bindings: [{ sequence: ['j'], label: 'J' }],
    }),
    action({ id: 'unbound', title: 'No keys' }),
  ])
  expect(sections.map(({ title }) => title)).toEqual([
    'General',
    'Navigation',
    'This page',
    'Lists',
    'Command palette',
  ])
  expect(sections[0].rows).toEqual([{ title: 'Open command palette', keys: ['⌘K'] }])
  expect(sections[1].rows).toEqual([{ title: 'Open Home', keys: ['⌘1', 'g h'] }])
  expect(sections[2].rows).toEqual([{ title: 'Show Week', keys: ['W'] }])
  expect(sections.flatMap(({ rows }) => rows).some(({ title }) => title === 'No keys')).toBe(false)
  expect(
    sections.flatMap(({ rows }) => rows).some(({ title }) => title.startsWith('Next item in')),
  ).toBe(false)
})
