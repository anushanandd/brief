import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { renderToString } from 'react-dom/server'
import { expect, it } from 'vitest'

import { emptySnapshot, transaction } from '../data/fixtures/records'
import { FinanceProvider } from '../hooks/finance-provider'
import type { FinanceSnapshot, Transaction } from '../lib/schema'
import { CategorySuggestionFlag } from './transaction-details-sections'

function render(data: FinanceSnapshot, shown: Transaction) {
  const client = new QueryClient()
  const html = renderToString(
    <QueryClientProvider client={client}>
      <FinanceProvider>
        <CategorySuggestionFlag data={data} transaction={shown} />
      </FinanceProvider>
    </QueryClientProvider>,
  )
  client.clear()
  // Text runs read as one, without React's markers between them.
  return html.replaceAll('<!-- -->', '')
}

const earlier = (id: string, date: string) =>
  transaction({ id, date, amount: -20, merchant: 'Example Bakery', categorySource: 'manual' })

it('says how sure a suggested category is, what it followed, and how to keep or decline it', () => {
  const suggested = transaction({
    id: 'c',
    date: '2026-09-01',
    amount: -20,
    merchant: 'Example Bakery',
    category: 'Dining',
    categorySource: 'suggested',
    providerCategory: 'Shopping',
    categorySuggestion: { confidence: 2 / 3, basis: 'merchant', from: ['b', 'a'] },
  })
  const data = {
    ...emptySnapshot(),
    transactions: [earlier('a', '2026-07-01'), earlier('b', '2026-08-01'), suggested],
  }
  const html = render(data, suggested)
  expect(html).toContain('Brief chose Dining, 67% sure, from your 2 earlier choices here')
  expect(html).toContain('The bank says Shopping.')
  expect(html).toContain('data-inspector-key="accept"')
  expect(html).toContain('data-inspector-key="reject"')
  // A category that is yours or the bank's has nothing to keep or decline.
  expect(render(data, earlier('a', '2026-07-01'))).toBe('')
})
