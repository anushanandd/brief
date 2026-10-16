import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderToString } from 'react-dom/server'
import { expect, it } from 'vitest'

import type { BankStatement } from '../lib/schema'
import { AccountStatements, statementMonth } from './account-statements'

const statement = (id: string, accountId: string, month: number): BankStatement => ({
  id,
  itemId: 'item-synthetic',
  accountId,
  accountName: 'Synthetic Checking',
  year: 2026,
  month,
  datePosted: `2026-${String(month).padStart(2, '0')}-28`,
  fetchedAt: '2026-10-12T12:00:00Z',
  bytes: 412_000,
})

function render(statements: BankStatement[], accountId: string) {
  const client = new QueryClient()
  client.setQueryData(['bank-statements'], statements)
  const html = renderToString(
    <QueryClientProvider client={client}>
      <AccountStatements accountId={accountId} />
    </QueryClientProvider>,
  )
  client.clear()
  return html
}

it('lists only this account’s statements, each a row that opens it', () => {
  const html = render(
    [
      statement('august', 'plaid:checking', 8),
      statement('july', 'plaid:checking', 7),
      statement('savings', 'plaid:savings', 8),
    ],
    'plaid:checking',
  )
  expect(html).toContain('Statements <small>2</small>')
  expect(html).toContain('August 2026')
  expect(html).toContain('July 2026')
  expect(html).toContain('412 KB')
  expect(html).toContain('data-command="account.statements.toggle"')
  expect(html.match(/data-keyboard-row/g)).toHaveLength(2)
})

it('offers an import for an account without statements', () => {
  const html = render([statement('savings', 'plaid:savings', 8)], 'plaid:checking')
  expect(html).not.toContain('Statements <small>')
  expect(html).not.toContain('August 2026')
  expect(html).toContain('data-command="account.statements.import"')
  expect(html).toContain('Import statement…')
})

it('names a statement by its month', () => {
  expect(statementMonth({ year: 2026, month: 1 })).toBe('January 2026')
  expect(statementMonth({ year: 2025, month: 12 })).toBe('December 2025')
})
