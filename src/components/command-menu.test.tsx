// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, useMemo } from 'react'
import { createRoot } from 'react-dom/client'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { ActionProvider, useRegisterActions, type AppAction } from '../lib/action-system'
import { datePresets } from '../lib/palette-filters'
import { heldSecurities, paletteQuestion } from '../lib/palette-rows'
import { CommandMenu } from './command-menu'

const scenario = vi.hoisted(() => ({ refreshing: false, withActivity: false }))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => vi.fn() }))
vi.mock('../hooks/use-finance', () => ({
  useFinance: () => ({
    data: {
      accounts: scenario.withActivity
        ? [{ id: 'synthetic-card', name: 'Synthetic Card', institution: 'Bank', type: 'credit' }]
        : [],
      transactions: scenario.withActivity
        ? ['2026-09-03', '2026-09-02'].map((date, index) => ({
            id: `synthetic-chipotle-${index}`,
            merchant: 'Chipotle',
            accountId: 'synthetic-card',
            account: 'Synthetic Card',
            category: 'Dining',
            date,
            amount: -20,
            pending: false,
            classification: {
              credit: false,
              kind: 'expense',
              mark: 'initial',
              spending: true,
              zelle: false,
            },
          }))
        : [],
      trades: scenario.withActivity
        ? [
            {
              id: 'synthetic-buy',
              type: 'BUY',
              accountId: 'synthetic-account',
              account: 'Synthetic Brokerage',
              ticker: 'NVDA',
              date: '2026-09-03',
              amount: -100,
            },
          ]
        : [],
      holdings: scenario.withActivity ? [{ ticker: 'NVDA', name: 'Synthetic Security' }] : [],
      updatedAt: '2026-09-03T12:00:00Z',
    },
  }),
}))
vi.mock('../hooks/use-refresh-finance', () => ({
  useRefreshFinance: () => vi.fn(),
  useFinanceRefreshState: () => scenario.refreshing,
}))

function RegisteredMenu() {
  const actions = useMemo<AppAction[]>(
    () => [
      {
        id: 'appearance.theme.dark',
        title: 'Switch to Dark theme',
        search: 'dark mode appearance',
        group: 'Appearance',
        scope: 'app',
        run: vi.fn(),
      },
    ],
    [],
  )
  useRegisterActions(actions)
  return <CommandMenu open onOpenChange={vi.fn()} onShowShortcuts={vi.fn()} />
}

beforeEach(() => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: { getItem: () => null, setItem: () => undefined, removeItem: () => undefined },
  })
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true
  }
  HTMLDialogElement.prototype.close = function () {
    this.open = false
  }
  Element.prototype.scrollIntoView = vi.fn()
})

afterEach(() => {
  scenario.refreshing = false
  scenario.withActivity = false
})

const renderMenu = () =>
  renderToStaticMarkup(
    <QueryClientProvider client={new QueryClient()}>
      <CommandMenu open onOpenChange={vi.fn()} onShowShortcuts={vi.fn()} />
    </QueryClientProvider>,
  )

async function mountMenu(
  element = <CommandMenu open onOpenChange={vi.fn()} onShowShortcuts={vi.fn()} />,
) {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  await act(async () => {
    root.render(
      <ActionProvider>
        <QueryClientProvider client={new QueryClient()}>{element}</QueryClientProvider>
      </ActionProvider>,
    )
  })
  const input = () => container.querySelector<HTMLInputElement>('[cmdk-input]')!
  const type = async (value: string) =>
    act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(
        input(),
        value,
      )
      input().dispatchEvent(new Event('input', { bubbles: true }))
    })
  const press = async (key: string, init: KeyboardEventInit = {}) =>
    act(async () => {
      input().dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, ...init }))
    })
  const labels = () =>
    [...container.querySelectorAll('[cmdk-item]')].map(
      (item) => item.querySelector('.command-item-copy > span')?.textContent ?? '',
    )
  const selectedLabel = () =>
    container.querySelector('[cmdk-item][data-selected="true"] .command-item-copy > span')
      ?.textContent
  const footer = () => container.querySelector('.command-footer')?.textContent ?? ''
  const activeTab = () => container.querySelector('[role="tab"][aria-selected="true"]')?.textContent
  const cleanup = async () => {
    await act(async () => root.unmount())
    container.remove()
  }
  return { container, input, type, press, labels, selectedLabel, footer, activeTab, cleanup }
}

it('renders an accessible combobox linked to its list of commands', () => {
  const html = renderMenu()
  const listId = html.match(/aria-controls="([^"]+)"/)?.[1]
  expect(listId).toBeTruthy()
  expect(html).toContain('role="combobox"')
  expect(html).toContain('aria-expanded="true"')
  expect(html).toMatch(new RegExp(`role="listbox"[^>]*id="${listId}"`))
  expect(html).toContain('role="option"')
  expect(html).toContain('Refresh snapshot')
  expect(html).toContain('Keyboard shortcuts')
  expect(html).not.toContain('Review data sources')
  expect(html).toContain('aria-label="Close"')
})

it('marks refresh unavailable while one is running', () => {
  scenario.refreshing = true
  const html = renderMenu()
  expect(html).toContain('Refreshing snapshot…')
  expect(html).toMatch(/aria-busy="true"[^>]*aria-disabled="true"[^>]*data-disabled="true"/)
})

it('lists each held ticker once with its searchable security name', () => {
  const holdings = [
    { ticker: 'SYN', name: 'Synthetic Fund' },
    { ticker: 'SYN', name: 'Synthetic Fund' },
    { ticker: 'ALT', name: 'Alternate Fund' },
  ]
  expect(heldSecurities(holdings)).toEqual([
    { ticker: 'SYN', name: 'Synthetic Fund' },
    { ticker: 'ALT', name: 'Alternate Fund' },
  ])
})

it('offers date filters that end today', () => {
  expect(datePresets('2026-09-15')).toEqual([
    { label: 'Today', from: '2026-09-15', to: '2026-09-15' },
    { label: 'Past 7 days', from: '2026-09-09', to: '2026-09-15' },
    { label: 'Past 30 days', from: '2026-08-17', to: '2026-09-15' },
    { label: 'This month', from: '2026-09-01', to: '2026-09-15' },
    { label: 'This year', from: '2026-01-01', to: '2026-09-15' },
  ])
})

it('keeps the question after a leading ?', () => {
  expect(paletteQuestion('dividends')).toBeNull()
  expect(paletteQuestion('  ? Why did dividends rise?  ')).toBe('Why did dividends rise?')
})

it('ranks an exact ticker, then Activity search, then matching entries', async () => {
  scenario.withActivity = true
  const menu = await mountMenu()
  await menu.type('nvda')
  const labels = menu.labels()
  const holding = labels.indexOf('NVDA · Synthetic Security')
  const search = labels.indexOf('Search Activity for “nvda”')
  const purchase = labels.indexOf('Bought NVDA')
  expect(holding).toBe(0)
  expect(search).toBeGreaterThan(holding)
  expect(purchase).toBeGreaterThan(search)
  await menu.type('chipotle')
  expect(menu.selectedLabel()).toBe('Search Activity for “chipotle”')
  expect(menu.labels()[1]).toBe('Chipotle')
  await menu.cleanup()
})

it('keeps the top result selected and in view for a one-letter query', async () => {
  const page = document.createElement('section')
  page.setAttribute('aria-label', 'Holdings')
  page.setAttribute('data-keyboard-region', '')
  page.innerHTML = '<button>Add account</button>'
  document.body.appendChild(page)
  const menu = await mountMenu()
  const list = menu.container.querySelector<HTMLElement>('[cmdk-list]')!
  list.scrollTop = 120
  await menu.type('a')
  expect(menu.container.textContent).not.toContain('Focus Holdings')
  expect(menu.container.textContent).not.toContain('Activate Add account')
  expect(menu.container.querySelector('[cmdk-item]')?.getAttribute('data-selected')).toBe('true')
  expect(list.scrollTop).toBe(0)
  await menu.cleanup()
  page.remove()
})

it('completes the selected row one segment at a time', async () => {
  const menu = await mountMenu(<RegisteredMenu />)
  expect(menu.container.textContent).not.toContain('Switch to Dark theme')
  await menu.type('Sw')
  expect(menu.container.querySelector('.autofill-ghost')?.textContent).toBe('Switch to Dark theme')
  await menu.press('Tab')
  expect(menu.input().value).toBe('Switch ')
  await menu.cleanup()
})

it('starts with Recents and Actions and shows a static key footer', async () => {
  const menu = await mountMenu()
  const headings = [...menu.container.querySelectorAll('[cmdk-group-heading]')].map(
    (heading) => heading.textContent,
  )
  expect(headings).toEqual(['Actions'])
  expect(menu.labels()).toEqual(['Refresh snapshot', 'Keyboard shortcuts'])
  expect(menu.footer()).toBe('CloseEscChange type←→Filters/Actions⌥↵')
  await menu.cleanup()
})

it('changes type with the arrow keys and with a leading prefix', async () => {
  scenario.withActivity = true
  const menu = await mountMenu()
  expect(menu.activeTab()).toBe('All')
  await menu.press('ArrowRight')
  expect(menu.activeTab()).toBe('Commands')
  await menu.press('ArrowRight')
  await menu.press('ArrowRight')
  expect(menu.activeTab()).toBe('Holdings')
  expect(menu.labels()).toEqual(['NVDA · Synthetic Security'])
  await menu.press('ArrowLeft')
  await menu.press('ArrowLeft')
  await menu.press('ArrowLeft')
  expect(menu.activeTab()).toBe('All')
  await menu.type('@syn')
  expect(menu.activeTab()).toBe('Accounts')
  expect(menu.input().value).toBe('syn')
  expect(menu.labels()).toEqual(['Synthetic Card'])
  await menu.type('zzz')
  expect(menu.container.querySelector('.command-empty')?.textContent).toContain(
    'No results in Accounts',
  )
  expect(menu.container.querySelector('.command-empty-action')?.textContent).toBe('Search all')
  await menu.cleanup()
})

it('filters Activity with / and removes a chip with Backspace', async () => {
  scenario.withActivity = true
  const menu = await mountMenu()
  await menu.type('/')
  expect(menu.input().value).toBe('')
  expect(menu.labels()).toEqual([
    'Filter by Account',
    'Filter by Category',
    'Filter by Method',
    'Filter by Date',
  ])
  expect(menu.footer()).toBe('CloseEscBack⌫Choose↵')
  await menu.press('ArrowDown')
  await menu.press('Enter')
  expect(menu.labels()).toEqual(['Dining', 'Trade'])
  await menu.press('Enter')
  expect(menu.container.querySelector('.command-chip')?.textContent).toBe('Dining')
  expect(menu.activeTab()).toBe('Activity')
  expect(menu.labels()).toEqual(['Show filtered Activity', 'Chipotle', 'Chipotle'])
  expect(menu.container.querySelector('[cmdk-item] small')?.textContent).toBe('2 matching entries')
  await menu.press('Backspace')
  expect(menu.container.querySelector('.command-chip')).toBeNull()
  await menu.cleanup()
})

it('opens a numbered Actions menu beside the selected row', async () => {
  scenario.withActivity = true
  const menu = await mountMenu()
  await menu.type('nvda')
  await menu.press('Enter', { altKey: true })
  const popover = menu.container.querySelector('.command-menu-popover')
  expect(popover?.querySelector('.command-menu-header')?.textContent).toBe(
    'NVDA · Synthetic SecurityHolding',
  )
  expect(
    [...(popover?.querySelectorAll('[role="menuitem"]') ?? [])].map((item) => item.textContent),
  ).toEqual([
    'Open holding1',
    'Search Activity for NVDA2',
    'Copy ticker3',
    'Add to Favorites4',
    'Reset ranking5',
  ])
  await menu.press('Escape')
  expect(menu.container.querySelector('.command-menu-popover')).toBeNull()
  expect(menu.input().value).toBe('nvda')
  await menu.press('Enter', { altKey: true })
  await menu.press('4')
  expect(menu.footer()).toBe('Added to Favorites')
  await menu.type('')
  const favorites = menu.container.querySelector('[cmdk-group]')
  expect(favorites?.querySelector('[cmdk-group-heading]')?.textContent).toBe('Favorites')
  expect(favorites?.textContent).toContain('NVDA · Synthetic Security')
  await menu.cleanup()
})

it('switches to Ask with ? and only offers Ask ↵ when Brief can answer', async () => {
  scenario.withActivity = true
  const menu = await mountMenu()
  await menu.type('?why')
  expect(menu.container.querySelector('.command-modes [aria-pressed="true"]')?.textContent).toBe(
    'Ask',
  )
  expect(menu.input().value).toBe('why')
  expect(menu.container.querySelector('.command-ask-status')?.textContent).toBe(
    'Apple Intelligence requires the desktop app',
  )
  expect(menu.footer()).not.toContain('Ask')
  await menu.type('')
  await menu.press('Backspace')
  expect(menu.container.querySelector('.command-modes [aria-pressed="true"]')?.textContent).toBe(
    'Search',
  )
  await menu.cleanup()
})

it('steps back one level per Esc before closing', async () => {
  const onOpenChange = vi.fn()
  const menu = await mountMenu(
    <CommandMenu open onOpenChange={onOpenChange} onShowShortcuts={vi.fn()} />,
  )
  await menu.type('/')
  await menu.press('Escape')
  expect(menu.container.querySelector('.command-tabs')).not.toBeNull()
  await menu.type('refresh')
  await menu.press('Escape')
  expect(menu.input().value).toBe('')
  expect(onOpenChange).not.toHaveBeenCalled()
  await menu.cleanup()
})
