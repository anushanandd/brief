import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { account } from '../data/fixtures/records'
import { creditMetrics } from './account-metrics'

const card = account({
  id: 'card',
  name: 'Synthetic Card',
  type: 'credit',
  liability: {
    lastStatementDate: null,
    lastStatementBalance: null,
    minimumPayment: null,
    nextPaymentDueDate: null,
    lastPaymentDate: null,
    lastPaymentAmount: null,
    purchaseApr: 21.5,
    isOverdue: null,
    aprs: [
      { type: 'purchase_apr', percentage: 21.5, balanceSubjectToApr: null, interestCharged: null },
      { type: 'cash_apr', percentage: null, balanceSubjectToApr: null, interestCharged: null },
    ],
  },
})

const metric = (html: string, label: string) =>
  html.match(new RegExp(`<span>${label}</span><strong[^>]*>(.*?)</strong>`))?.[1]

it("leaves a card's month unknown, not $0, without Rust's spending period", () => {
  const html = renderToStaticMarkup(
    <>
      {creditMetrics({
        account: card,
        spending: undefined,
        largest: undefined,
        transactionCount: 0,
        card: undefined,
      })}
    </>,
  )
  expect(metric(html, 'Spent this month')).toBe('—')
  expect(metric(html, 'Pending')).toBe('—')
  // A rate the provider didn't send is left out rather than read.
  expect(html).toContain('21.50%')
  expect(html).not.toContain('cash ')
})
