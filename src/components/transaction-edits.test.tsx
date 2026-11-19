// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { renderToString } from 'react-dom/server'
import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { emptySnapshot, transaction } from '../data/fixtures/records'
import { FinanceProvider } from '../hooks/finance-provider'
import type { FinanceSnapshot, Transaction } from '../lib/schema'
import { TransactionEdits } from './transaction-edits'

function render(data: FinanceSnapshot, shown: Transaction) {
  const client = new QueryClient()
  const html = renderToString(
    <QueryClientProvider client={client}>
      <FinanceProvider>
        <TransactionEdits data={data} transaction={shown} />
      </FinanceProvider>
    </QueryClientProvider>,
  )
  client.clear()
  return html.replaceAll('<!-- -->', '')
}

const tickets = transaction({
  id: 'tickets',
  date: '2026-09-12',
  amount: -150,
  merchant: 'Example venue',
  classification: classification('expense'),
})
const payback = (id: string, patch: Partial<Transaction> = {}) =>
  transaction({
    id,
    date: '2026-09-14',
    amount: 50,
    merchant: `Zelle from ${id}`,
    accountId: 'checking',
    classification: classification('transfer'),
    ...patch,
  })

it('lists a purchase’s paybacks, the share left, and the way to link another', () => {
  const data = {
    ...emptySnapshot(),
    transactions: [tickets, payback('Alex', { refundOf: 'tickets' }), payback('Sam')],
  }
  const html = render(data, tickets)
  expect(html).toContain('<span>Paid back</span>')
  expect(html).toContain('Zelle from Alex on Sep 14, 2026 · $50.00')
  expect(html).toContain('Your share <span class="sensitive-value">−$100.00</span>')
  expect(html).toContain('aria-label="Link a payback"')

  // With nothing linked yet, the way to link waits in the Add line.
  const unlinked = render({ ...data, transactions: [tickets, payback('Sam')] }, tickets)
  expect(unlinked).not.toContain('<span>Paid back</span>')
  expect(unlinked).toMatch(/transaction-edit-add[\s\S]*aria-keyshortcuts="B"[\s\S]*Payback/)
})

it('folds empty tags, people, and note into one line, with split beside the category', () => {
  const data = { ...emptySnapshot(), transactions: [tickets] }
  const html = render(data, tickets)
  expect(html).not.toContain('aria-label="Add a tag"')
  expect(html).not.toContain('aria-label="Note"')
  expect(html).toMatch(/transaction-edit-add[\s\S]*Tag[\s\S]*Person[\s\S]*Note/)
  expect(html).toContain('data-inspector-key="split"')
  expect(html).not.toContain('<span>Paid back</span>')
  // A tag already saved shows its row.
  expect(render(data, { ...tickets, tags: ['Concert'] })).toContain('aria-label="Add a tag"')
})

it('keeps a typed note when saving it fails', async () => {
  const noted = { ...tickets, note: 'Old note' }
  const client = new QueryClient()
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () =>
    root.render(
      <QueryClientProvider client={client}>
        <FinanceProvider>
          <TransactionEdits
            data={{ ...emptySnapshot(), transactions: [noted] }}
            transaction={noted}
          />
        </FinanceProvider>
      </QueryClientProvider>,
    ),
  )
  const field = document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Note"]')!
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set?.call(
      field,
      'New note',
    )
    field.dispatchEvent(new Event('input', { bubbles: true }))
  })
  // Outside the desktop app every edit fails to save.
  await act(async () => {
    field.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true }))
  })
  expect(field.value).toBe('New note')
  expect(document.body.textContent).toContain('Transaction edits require the Brief desktop app')
  await act(async () => root.unmount())
  client.clear()
  document.body.replaceChildren()
})
