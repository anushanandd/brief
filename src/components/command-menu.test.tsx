// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import {
  CommandMenu,
  heldSecurities,
  paletteDateRange,
  paletteFilter,
  paletteQuestion,
} from './command-menu'

const scenario = vi.hoisted(() => ({ refreshing: false, withActivity: false }))
vi.mock('@tanstack/react-router', () => ({ useNavigate: () => vi.fn() }))
vi.mock('../hooks/use-finance', () => ({
  useFinance: () => ({
    data: {
      accounts: [],
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
              brokerageIncomeTransfer: false,
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
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', { value: true, configurable: true })
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
  expect(html).toContain('? to ask')
  expect(html).not.toContain('Try description:coffee')
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

it('puts exact held tickers before Activity search and individual entries', () => {
  expect(paletteFilter('holding:NVDA', 'nvda', ['NVDA · NVIDIA'])).toBe(2)
  expect(paletteFilter('holding:NVDA', 'nvidia', ['NVDA · NVIDIA'])).toBe(1)
  expect(paletteFilter('navigate:/logs', 'nvda', ['Diagnostics', 'navigate Diagnostics'])).toBe(0)
  expect(paletteFilter('activity:search:nvda', 'nvda', ['Search Activity for NVDA'])).toBe(1.5)
  expect(paletteFilter('activity:purchase', 'nvda', ['Purchase of NVDA'])).toBe(0.5)
  expect(paletteFilter('activity-filter:category:Coffee', 'coffee', ['Coffee'])).toBe(0.5)
})

it('shows and selects Activity search before matching merchant purchases', async () => {
  scenario.withActivity = true
  HTMLDialogElement.prototype.showModal = function () {
    this.open = true
  }
  HTMLDialogElement.prototype.close = function () {
    this.open = false
  }
  Element.prototype.scrollIntoView = vi.fn()
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  await act(async () => {
    root.render(
      <QueryClientProvider client={new QueryClient()}>
        <CommandMenu open onOpenChange={vi.fn()} onShowShortcuts={vi.fn()} />
      </QueryClientProvider>,
    )
  })
  const input = container.querySelector<HTMLInputElement>('[cmdk-input]')!
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(input, 'nvda')
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
  const labels = [...container.querySelectorAll('[cmdk-item]')]
    .filter((item) => !item.closest('[hidden]'))
    .map((item) => item.textContent)
  const holding = labels.findIndex((label) => label?.includes('NVDA · Synthetic Security'))
  const search = labels.findIndex((label) => label?.includes('Search Activity for “nvda”'))
  const purchase = labels.findIndex((label) => label?.includes('Bought NVDA'))
  expect(holding).toBeGreaterThanOrEqual(0)
  expect(search).toBeGreaterThan(holding)
  expect(purchase).toBeGreaterThan(search)
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set?.call(
      input,
      'chipotle',
    )
    input.dispatchEvent(new Event('input', { bubbles: true }))
  })
  const visible = [...container.querySelectorAll<HTMLElement>('[cmdk-item]')].filter(
    (item) => !item.closest('[hidden]'),
  )
  expect(visible[0].textContent).toContain('Search Activity for “chipotle”')
  expect(visible[0].getAttribute('data-selected')).toBe('true')
  expect(visible[1].textContent).toContain('Chipotle')
  await act(async () => root.unmount())
  container.remove()
})

it('accepts real single dates and ordered date ranges for Activity filters', () => {
  expect(paletteDateRange('date:2026-09-03')).toEqual({
    from: '2026-09-03',
    to: '2026-09-03',
  })
  expect(paletteDateRange('2026-09-01..2026-09-30')).toEqual({
    from: '2026-09-01',
    to: '2026-09-30',
  })
  expect(paletteDateRange('date:2026-09-31')).toBeUndefined()
  expect(paletteDateRange('date:2026-09-30..2026-09-01')).toBeUndefined()
})

it('requires an explicit question prefix', () => {
  expect(paletteQuestion('dividends')).toBeNull()
  expect(paletteQuestion('  ? Why did dividends rise?  ')).toBe('Why did dividends rise?')
  expect(paletteQuestion('?')).toBe('')
})
