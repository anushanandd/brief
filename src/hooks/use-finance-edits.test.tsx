// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { TransactionEdits } from '../components/transaction-edits'
import { seedSnapshot } from '../data/fixtures/records'
import { restoreTransactionAnnotations, saveTransactionAnnotations } from '../lib/annotations'
import type { FinanceSnapshot } from '../lib/schema'
import { forgetEdits } from '../lib/undo'
import { financeQueryKey } from './use-finance'
import { useGlobalActions } from './use-global-actions'
import { useKeyboardDispatch } from './use-keyboard-dispatch'

vi.mock('@tanstack/react-router', () => {
  const navigate = vi.fn()
  return { useNavigate: () => navigate, useLocation: () => '/' }
})
// The provider's state, reduced to the cached snapshot every writer updates.
vi.mock('./use-finance', async (original) => {
  const actual = await original<typeof import('./use-finance')>()
  const { useQuery } = await import('@tanstack/react-query')
  return {
    ...actual,
    useFinance: () => ({
      data: useQuery<FinanceSnapshot>({ queryKey: actual.financeQueryKey, enabled: false }).data,
      integrationStatus: { plaid: false },
    }),
  }
})
vi.mock('./use-refresh-finance', () => {
  const refresh = vi.fn()
  return { useRefreshFinance: () => refresh, useFinanceRefreshState: () => false }
})
vi.mock('../lib/navigation-preferences', () => {
  const pages: never[] = []
  return { useOrderedNavigation: () => pages }
})
vi.mock('../lib/settings-dialog', () => {
  const show = vi.fn()
  return { useSettingsDialog: () => ({ open: false, target: '', show, close: vi.fn() }) }
})
// The native commands; each returns the annotation-aware snapshot Rust saved.
vi.mock('../lib/annotations', async (original) => ({
  ...(await original<typeof import('../lib/annotations')>()),
  saveTransactionAnnotations: vi.fn(),
  restoreTransactionAnnotations: vi.fn(),
}))

const controls = {
  openPalette: () => {},
  openShortcuts: () => {},
  openKeyGuide: () => {},
  openFocusHints: () => {},
  valuesHidden: false,
  toggleValues: () => {},
}

/** The app's shortcuts, as the shell dispatches them. */
function Shortcuts() {
  useKeyboardDispatch(useGlobalActions(controls), undefined, false)
  return null
}

const before: FinanceSnapshot = { ...seedSnapshot(), revision: 4 }
const [transaction] = before.transactions
if (!transaction) throw new Error('The seed has a transaction')
const noted = (note: string | undefined): FinanceSnapshot => ({
  ...before,
  transactions: before.transactions.map((item) =>
    item.id === transaction.id ? { ...item, note } : item,
  ),
})
const heldNote = () =>
  client
    .getQueryData<FinanceSnapshot>(financeQueryKey)
    ?.transactions.find(({ id }) => id === transaction.id)?.note

let client: QueryClient
let root: Root
let container: HTMLElement

beforeEach(async () => {
  client = new QueryClient()
  client.setQueryData(financeQueryKey, before)
  container = document.body.appendChild(document.createElement('div'))
  root = createRoot(container)
  await act(async () =>
    root.render(
      <QueryClientProvider client={client}>
        <Shortcuts />
        <TransactionEdits transaction={transaction} data={before} />
      </QueryClientProvider>,
    ),
  )
})

afterEach(async () => {
  await act(async () => root.unmount())
  document.body.replaceChildren()
  forgetEdits()
  vi.mocked(saveTransactionAnnotations).mockReset()
  vi.mocked(restoreTransactionAnnotations).mockReset()
})

/** Types a note and leaves the field, which saves it. */
async function writeNote(text: string) {
  const field = container.querySelector<HTMLTextAreaElement>('textarea[aria-label="Note"]')
  if (!field) throw new Error('The note field renders')
  await act(async () => {
    field.focus()
    // React tracks the value it set; the prototype's setter changes it as typing would.
    Reflect.set(HTMLTextAreaElement.prototype, 'value', text, field)
    field.dispatchEvent(new Event('input', { bubbles: true }))
  })
  await act(async () => field.blur())
}

it('applies the snapshot an edit returns, and ⌘Z restores the one before', async () => {
  vi.mocked(saveTransactionAnnotations).mockResolvedValue({
    annotations: { [transaction.id]: { note: 'Synthetic note' } },
    previous: { [transaction.id]: {} },
    snapshot: noted('Synthetic note'),
  })
  vi.mocked(restoreTransactionAnnotations).mockResolvedValue({
    annotations: {},
    previous: {},
    snapshot: noted(undefined),
  })

  await writeNote('Synthetic note')
  expect(saveTransactionAnnotations).toHaveBeenCalledWith({
    [transaction.id]: { note: 'Synthetic note' },
  })
  expect(heldNote()).toBe('Synthetic note')

  await act(async () => {
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'z', metaKey: true, bubbles: true, cancelable: true }),
    )
  })
  expect(restoreTransactionAnnotations).toHaveBeenCalledWith({ [transaction.id]: {} })
  expect(heldNote()).toBeUndefined()
})

it('shows a failed edit’s error and keeps the snapshot as it was', async () => {
  vi.mocked(saveTransactionAnnotations).mockRejectedValue(new Error('Synthetic save failure'))

  await writeNote('Synthetic note')
  expect(container.querySelector('[role="alert"]')?.textContent).toBe('Synthetic save failure')
  expect(client.getQueryData(financeQueryKey)).toBe(before)

  // Nothing was remembered, so ⌘Z has nothing to restore.
  await act(async () => {
    document.body.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'z', metaKey: true, bubbles: true, cancelable: true }),
    )
  })
  expect(restoreTransactionAnnotations).not.toHaveBeenCalled()
})
