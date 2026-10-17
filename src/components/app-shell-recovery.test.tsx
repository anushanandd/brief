// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, type ReactNode } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import empty from '../data/empty-view.json'
import { emptySnapshot } from '../data/fixtures/records'
import { recoverFinanceState } from '../lib/api'
import { AppShell } from './app-shell'

const snapshot = emptySnapshot()

vi.mock('@tanstack/react-router', () => ({
  Link: ({ to, children }: { to: string; children?: ReactNode }) => <a href={to}>{children}</a>,
  Outlet: () => null,
  useNavigate: () => vi.fn(),
  useLocation: () => '/',
  useRouter: () => ({ routesByPath: {}, navigate: vi.fn() }),
}))
vi.mock('../hooks/use-finance', () => ({
  financeQueryKey: ['finance'],
  useFinance: () => ({
    data: { ...empty, recovery: { message: 'Synthetic recovery notice', canRestore: true } },
    integrationStatus: {},
    integrationStatusLoading: false,
  }),
}))
vi.mock('../hooks/use-refresh-finance', () => ({
  useAutoRefreshFinance: () => undefined,
  useFinanceRefreshState: () => false,
  useRefreshFinance: () => vi.fn(),
}))
vi.mock('../lib/navigation-preferences', () => ({
  useOrderedNavigation: () => [],
  adjacentPage: () => '/',
  reportsGroupOpen: {
    key: 'brief:sidebar-analytics-open',
    get: () => false,
    save: () => {},
    use: () => false,
  },
}))
vi.mock('../lib/api', () => ({ isTauri: () => false, recoverFinanceState: vi.fn() }))
vi.mock('next-themes', () => ({ useTheme: () => ({ theme: 'system', setTheme: vi.fn() }) }))

let root: Root
let client: QueryClient

beforeEach(async () => {
  vi.mocked(recoverFinanceState).mockReset()
  client = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () =>
    root.render(
      <QueryClientProvider client={client}>
        <AppShell />
      </QueryClientProvider>,
    ),
  )
})

afterEach(async () => {
  await act(async () => root.unmount())
  client.clear()
  document.body.replaceChildren()
})

function button(label: string) {
  const element = [...document.querySelectorAll<HTMLButtonElement>('button')].find(
    (candidate) => candidate.textContent === label,
  )
  if (!element) throw new Error(`Button missing: ${label}`)
  return element
}

async function click(label: string) {
  await act(async () => button(label).click())
}

it('requires confirmation, focuses Cancel, and returns focus after cancellation or Escape', async () => {
  await click('Start new snapshot')
  expect(recoverFinanceState).not.toHaveBeenCalled()
  expect(document.activeElement).toBe(button('Cancel'))
  await click('Cancel')
  expect(document.querySelector('#new-snapshot-confirmation')).toBeNull()
  expect(document.activeElement).toBe(button('Start new snapshot'))
  await click('Start new snapshot')
  await act(async () =>
    button('Cancel').dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
    ),
  )
  expect(document.querySelector('#new-snapshot-confirmation')).toBeNull()
  expect(document.activeElement).toBe(button('Start new snapshot'))
  expect(recoverFinanceState).not.toHaveBeenCalled()
})

it('blocks duplicate recovery while pending, shows errors, and permits a confirmed retry', async () => {
  let rejectRecovery!: (reason: Error) => void
  vi.mocked(recoverFinanceState).mockImplementationOnce(
    () =>
      new Promise((_, reject) => {
        rejectRecovery = reject
      }),
  )
  await click('Start new snapshot')
  await click('Confirm start')
  await act(async () => {
    await vi.waitFor(() => expect(button('Starting…').disabled).toBe(true))
  })
  expect(vi.mocked(recoverFinanceState).mock.calls.map(([restore]) => restore)).toEqual([false])
  expect(button('Cancel').disabled).toBe(true)
  expect(button('Restore retained snapshot').disabled).toBe(true)
  expect(
    document.querySelector('[aria-label="Local data recovery"]')?.getAttribute('aria-busy'),
  ).toBe('true')
  await click('Starting…')
  expect(recoverFinanceState).toHaveBeenCalledTimes(1)
  await act(async () => rejectRecovery(new Error('Synthetic recovery failure')))
  await act(async () => {
    await vi.waitFor(() =>
      expect(document.querySelector('[role="alert"]')?.textContent).toContain(
        'Synthetic recovery failure',
      ),
    )
  })
  expect(button('Confirm start').disabled).toBe(false)
  vi.mocked(recoverFinanceState).mockResolvedValueOnce(snapshot)
  await click('Confirm start')
  await act(async () => {
    await vi.waitFor(() => expect(document.querySelector('#new-snapshot-confirmation')).toBeNull())
  })
  expect(vi.mocked(recoverFinanceState).mock.calls.map(([restore]) => restore)).toEqual([
    false,
    false,
  ])
  expect(client.getQueryData(['finance'])).toEqual(snapshot)
})

it('restores the retained snapshot through the existing recovery action', async () => {
  vi.mocked(recoverFinanceState).mockResolvedValueOnce(snapshot)
  await click('Restore retained snapshot')
  expect(vi.mocked(recoverFinanceState).mock.calls.map(([restore]) => restore)).toEqual([true])
})
