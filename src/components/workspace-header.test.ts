import { expect, it } from 'vitest'

import { formatUpdatedAt } from '../lib/format'
import { connectionStates, marketPriceLabel, marketSessionLabel } from './workspace-header'

it('names closed weekend sessions without relabeling other sessions or weekday closures', () => {
  expect(marketSessionLabel('Market closed', new Date('2026-09-26T16:00:00Z'))).toBe('Weekend')
  expect(marketSessionLabel('Market closed', new Date('2026-09-28T16:00:00Z'))).toBe(
    'Market closed',
  )
  expect(marketSessionLabel('After hours', new Date('2026-09-26T16:00:00Z'))).toBe('After hours')
})

it('omits the current year from saved timestamps', () => {
  const year = new Date().getFullYear()
  expect(formatUpdatedAt(`${year}-06-15T12:34:00`)).not.toContain(String(year))
  expect(formatUpdatedAt(`${year - 1}-06-15T12:34:00`)).toContain(String(year - 1))
})

it('reports each provider as not set up, connected, checked, or needing attention', () => {
  const states = connectionStates(
    { plaid: true, snaptrade: true, alpaca: true, alphaVantage: false, sec: true, exa: false },
    {
      plaid: { updatedAt: '2026-10-05T17:20:00Z', error: null },
      snaptrade: { updatedAt: '2026-10-05T17:20:00Z', error: 'Reauthorize' },
    },
    true,
  )
  expect(states.map(({ id, tone, state, checkedAt }) => ({ id, tone, state, checkedAt }))).toEqual([
    { id: 'plaid', tone: 'positive', state: 'Connected', checkedAt: '2026-10-05T17:20:00Z' },
    { id: 'snaptrade', tone: 'negative', state: 'Needs attention', checkedAt: undefined },
    { id: 'alpaca', tone: 'negative', state: 'Needs attention', checkedAt: undefined },
    { id: 'alphavantage', tone: 'neutral', state: 'Not set up', checkedAt: undefined },
    { id: 'sec', tone: 'positive', state: 'Connected', checkedAt: undefined },
    { id: 'exa', tone: 'neutral', state: 'Not set up', checkedAt: undefined },
  ])
})

it('never labels saved market prices live', () => {
  expect(marketPriceLabel('active', false, true)).toBe('Live')
  expect(marketPriceLabel('loading', true, true)).toBe('Saved · updating')
  expect(marketPriceLabel('idle', true, true)).toBe('Saved')
  expect(marketPriceLabel('error', true, true)).toBe('Unavailable')
  expect(marketPriceLabel('idle', false, false)).toBe('Off')
})
