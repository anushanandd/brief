import { expect, it } from 'vitest'

import type { AppAction } from './action-system'
import { commandMenuSections, menuAccelerator, openedViewHref, viewLink } from './native-commands'

const action = (overrides: Partial<AppAction>): AppAction => ({
  id: 'id',
  title: 'Title',
  scope: 'page',
  run: () => undefined,
  ...overrides,
})

it('gives menu key equivalents only to Command shortcuts that never edit text', () => {
  expect(menuAccelerator({ sequence: ['Meta+k'], label: '⌘K' })).toBe('Cmd+K')
  expect(menuAccelerator({ sequence: ['Meta+1'], label: '⌘1' })).toBe('Cmd+1')
  expect(menuAccelerator({ sequence: ['Meta+/'], label: '⌘/' })).toBe('Cmd+Slash')
  expect(menuAccelerator({ sequence: ['Meta+Control+r'], label: '⌘⌃R' })).toBe('Cmd+Ctrl+R')
  // Bare keys and Command arrows would fire while typing in a field.
  expect(menuAccelerator({ sequence: ['w'], label: 'W' })).toBeNull()
  expect(menuAccelerator({ sequence: ['Meta+ArrowLeft'], label: '⌘←' })).toBeNull()
  // Command-Control arrows step chart date views and edit no text.
  expect(menuAccelerator({ sequence: ['Meta+Control+ArrowLeft'], label: '⌘⌃←' })).toBe(
    'Cmd+Ctrl+ArrowLeft',
  )
  expect(menuAccelerator({ sequence: ['g', 'h'], label: 'g h' })).toBeNull()
  // The static menu already owns these.
  expect(menuAccelerator({ sequence: ['Meta+h'], label: '⌘H' })).toBeNull()
})

it('lists app and page commands once, by section, leaving focus-bound ones to the palette', () => {
  const sections = commandMenuSections([
    action({ id: 'navigation.open./', title: 'Open Home', group: 'Navigate', scope: 'app' }),
    action({ id: 'theme', title: 'Use Dark', group: 'Appearance', scope: 'app' }),
    action({ id: 'export', title: 'Export', disabled: true }),
    action({ id: 'motion.next:list', title: 'Next item', group: 'Here', scope: 'region' }),
    action({
      id: 'graph.range.graph-week',
      title: 'Show Week',
      group: 'View',
      bindings: [{ sequence: ['w'], label: 'W' }],
    }),
    action({
      id: 'privacy',
      title: 'Hide values',
      scope: 'app',
      bindings: [{ sequence: ['Meta+h'], label: '⌘H' }],
    }),
  ])
  expect(sections.map(({ title }) => title)).toEqual(['Go', 'View', 'Actions'])
  expect(sections.flatMap(({ items }) => items.map(({ id }) => id))).toEqual([
    'navigation.open./',
    'theme',
    'graph.range.graph-week',
    'export',
  ])
  // A bare key is no menu key equivalent; the page handles it outside fields.
  expect(sections[1].items[1].accelerator).toBeNull()
  expect(sections[2].items[0].enabled).toBe(false)
})

it('opens only real views from brief:// links', () => {
  const routes = ['/', '/accounts', '/holdings/$ticker', '/analytics']
  expect(openedViewHref('/accounts?account=checking', routes)).toBe('/accounts?account=checking')
  expect(openedViewHref('/holdings/VTI', routes)).toBe('/holdings/VTI')
  expect(openedViewHref('/settings/privacy', routes)).toBe('/#settings/privacy')
  expect(openedViewHref('/nowhere', routes)).toBeUndefined()
  expect(openedViewHref('javascript:alert(1)', routes)).toBeUndefined()
  expect(viewLink({ pathname: '/analytics', searchStr: '?chart=income' })).toBe(
    'brief://analytics?chart=income',
  )
})
