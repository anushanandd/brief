import { renderToStaticMarkup } from 'react-dom/server'
import { describe, expect, it } from 'vitest'

import { boundedTransactionTitle, CashFlowSankey, cashFlowSankeyModel } from './cash-flow-sankey'

describe('cash-flow Sankey model', () => {
  it('limits displayed transaction titles to five words', () => {
    expect(boundedTransactionTitle('Transfer money from a brokerage account')).toBe(
      'Transfer money from a brokerage…',
    )
    expect(boundedTransactionTitle('Zelle from Alex')).toBe('Zelle from Alex')
  })

  it('balances reimbursements and retained cash against purchases and fees', () => {
    const model = cashFlowSankeyModel({
      sources: [
        {
          id: 'source:Payroll',
          label: 'Payroll',
          value: 2_000,
          children: [
            { id: 'pay-1', label: 'Payroll', value: 1_000, date: '2026-09-01' },
            { id: 'pay-2', label: 'Payroll', value: 1_000, date: '2026-09-15' },
          ],
        },
        {
          id: 'source:Zelle from Alex',
          label: 'Zelle from Alex',
          value: 40,
          children: [{ id: 'zelle', label: 'Zelle', value: 40, date: '2026-09-12' }],
        },
      ],
      purchases: [
        {
          id: 'purchase:Dining',
          label: 'Dining',
          value: 120,
          children: [{ id: 'meal', label: 'Restaurant', value: 120, date: '2026-09-10' }],
        },
      ],
      fees: 5,
      taxes: 0,
    })

    expect(model.total).toBe(2_040)
    expect(model.uses.find(({ id }) => id === 'retained')?.value).toBe(1_915)
    expect(model.sources.reduce((total, flow) => total + flow.value, 0)).toBe(model.total)
    expect(model.uses.reduce((total, flow) => total + flow.value, 0)).toBe(model.total)
  })

  it('adds an explicit funding gap when classified outflows exceed inflows', () => {
    const model = cashFlowSankeyModel({
      sources: [
        {
          id: 'source:Payroll',
          label: 'Payroll',
          value: 100,
          children: [{ id: 'pay', label: 'Payroll', value: 100, date: '2026-09-01' }],
        },
      ],
      purchases: [
        {
          id: 'purchase:Dining',
          label: 'Dining',
          value: 150,
          children: [{ id: 'meal', label: 'Restaurant', value: 150, date: '2026-09-02' }],
        },
      ],
      fees: 0,
      taxes: 0,
    })

    expect(model.total).toBe(150)
    expect(model.sources.find(({ id }) => id === 'gap')?.value).toBe(50)
    expect(model.uses.some(({ id }) => id === 'retained')).toBe(false)
  })

  it('keeps the chart aggregated and exposes transaction detail on its labels and bands', () => {
    const html = renderToStaticMarkup(
      CashFlowSankey({
        values: {
          sources: [
            {
              id: 'source:Handshake',
              label: 'Handshake',
              value: 200,
              children: [
                { id: 'handshake-1', label: 'Payment', value: 120, date: '2026-09-01' },
                { id: 'handshake-2', label: 'Payment', value: 80, date: '2026-09-15' },
              ],
            },
          ],
          purchases: [
            {
              id: 'purchase:Dining',
              label: 'Dining',
              value: 150,
              children: [
                { id: 'cafe', label: 'Cafe', value: 100, date: '2026-09-05' },
                { id: 'deli', label: 'Deli', value: 50, date: '2026-09-06' },
              ],
            },
          ],
          fees: 0,
          taxes: 0,
        },
      }),
    )

    expect(html).toContain('sankey-label-source')
    expect(html).toContain('sankey-pool-label')
    expect(html).not.toContain('Cash available')
    expect(html).toContain('sankey-label-use')
    expect(html.match(/sankey-label-interactive/g)).toHaveLength(2)
    expect(html.match(/sankey-link-interactive/g)).toHaveLength(2)
    expect(html).toContain('Payment $120.00 on Sep 1')
    expect(html).toContain('Cafe $100.00 on Sep 5')
  })
})
