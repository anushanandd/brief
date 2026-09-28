import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import type { Account } from '../lib/schema'
import { SnapTradeReference } from './snaptrade-reference'

it('shows a dated reported value for individual SnapTrade accounts only', () => {
  const account: Account = {
    id: 'snaptrade:broker',
    name: 'Brokerage',
    institution: 'Broker',
    type: 'brokerage',
    value: 200,
    reportedBalance: 190,
    balanceAsOf: '2026-09-23T20:00:00Z',
  }
  expect(renderToStaticMarkup(<SnapTradeReference account={account} />)).toContain(
    'SnapTrade: $190.00 <time dateTime="2026-09-23T20:00:00Z">(Sep 23, 2026)</time>',
  )
  expect(
    renderToStaticMarkup(<SnapTradeReference account={{ ...account, id: 'snaptrade:ira' }} />),
  ).toContain('SnapTrade: $190.00')
  expect(
    renderToStaticMarkup(<SnapTradeReference account={{ ...account, reportedBalance: null }} />),
  ).toContain('SnapTrade: —')
  for (const id of ['all', 'total']) {
    expect(renderToStaticMarkup(<SnapTradeReference account={{ ...account, id }} />)).toBe('')
  }
})
