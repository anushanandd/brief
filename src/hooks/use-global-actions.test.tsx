// @vitest-environment jsdom

import { act, useState } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import type { AppAction } from '../lib/action-system'
import { useGlobalActions } from './use-global-actions'

vi.mock('@tanstack/react-router', () => {
  const navigate = vi.fn()
  return { useNavigate: () => navigate, useLocation: () => '/' }
})
vi.mock('next-themes', () => {
  const setTheme = vi.fn()
  return { useTheme: () => ({ theme: 'custom', setTheme }) }
})
vi.mock('./use-finance', () => ({
  useFinance: () => ({ data: undefined, integrationStatus: { plaid: false } }),
}))
vi.mock('./use-finance-edits', () => {
  const undo = vi.fn()
  return { useFinanceEdits: () => ({ undo, undoLabel: undefined }) }
})
vi.mock('./use-refresh-finance', () => {
  const refresh = vi.fn()
  return { useRefreshFinance: () => refresh, useFinanceRefreshState: () => false }
})
vi.mock('../lib/navigation-preferences', () => {
  // The real hook memoizes the order; the mock must be as stable.
  const pages: never[] = []
  return { useOrderedNavigation: () => pages, adjacentPage: () => '/' }
})
// Like the real hook: a fresh object each render around stable callbacks.
vi.mock('../lib/settings-dialog', () => {
  const show = vi.fn()
  return { useSettingsDialog: () => ({ open: false, target: '', show, close: vi.fn() }) }
})

const seen: AppAction[][] = []
let rerender: () => void = () => {}
const controls = {
  openPalette: () => {},
  openShortcuts: () => {},
  openKeyGuide: () => {},
  openFocusHints: () => {},
  valuesHidden: false,
  toggleValues: () => {},
  toggleReports: () => {},
}

function Probe() {
  const [, tick] = useState(0)
  rerender = () => tick((count) => count + 1)
  seen.push(useGlobalActions(controls))
  return null
}

let root: Root
beforeEach(() => {
  seen.length = 0
  root = createRoot(document.body.appendChild(document.createElement('div')))
})
afterEach(() => {
  act(() => root.unmount())
})

it('keeps the same action list across renders that change nothing it reads', () => {
  act(() => root.render(<Probe />))
  act(() => rerender())
  act(() => rerender())
  expect(seen.length).toBeGreaterThanOrEqual(3)
  // A new list each render would re-register with the action registry and loop the shell.
  expect(new Set(seen).size).toBe(1)
  expect(seen[0].some(({ id }) => id === 'settings.open')).toBe(true)
})
