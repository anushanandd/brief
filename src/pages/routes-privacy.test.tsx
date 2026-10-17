// Screen privacy fails closed: with values hidden, no page may show a personal amount.

import { expect, it } from 'vitest'

import { HIDDEN_AMOUNT } from '../lib/format'
import { renderRoute, renderSettings } from './routes.harness'
import { scenario } from './routes.scenario'

const holding = {
  ticker: 'AAA',
  name: 'Alpha fund',
  accountId: 'selected',
  shares: 2,
  price: 50,
  value: 100,
  costBasis: 80,
  unrealizedGain: 20,
  dailyChangePct: 1,
  weeklyChangePct: null,
  weeklyReferencePrice: null,
  weeklyReferenceDate: null,
  totalChangePct: 25,
  instrumentKind: 'equity',
  color: '#fff',
}

/** Removes public market prices and example text in field placeholders, neither of which is personal. */
const withoutPublicPrices = (html: string) =>
  html
    .replaceAll(/<(td|span)[^>]*class="public-market-value"[^>]*>[\s\S]*?<\/\1>/g, '')
    .replaceAll(/placeholder="[^"]*"/g, '')

/** Dollar amounts, with or without a sign, in text or in attributes such as labels. */
const amounts = (html: string) => html.match(/[+−-]?\$\s?\d[\d,]*(\.\d+)?/g) ?? []

it.each([
  '/',
  '/spending',
  '/accounts',
  '/activities',
  '/analytics',
  '/analytics?chart=income',
  '/insights',
  '/taxes',
  '/documents',
  '/calendar',
  '/holdings',
])('shows no personal amount on %s while values are hidden', async (path) => {
  scenario.valuesHidden = true
  scenario.holdings = [holding]
  const html = await renderRoute(path)
  expect(amounts(withoutPublicPrices(html))).toEqual([])
})

it('masks amounts in Settings while values are hidden', async () => {
  scenario.valuesHidden = true
  const html = await renderSettings('/#settings/accounts')
  expect(amounts(html)).toEqual([])
})

it('replaces hidden amounts with one placeholder and keeps public prices visible', async () => {
  scenario.valuesHidden = true
  scenario.holdings = [holding]
  const html = await renderRoute('/')
  expect(html).toContain(HIDDEN_AMOUNT)
  expect(html).toContain('<td class="public-market-value">$50.00</td>')
})

it('shows amounts again once values are visible', async () => {
  scenario.holdings = [holding]
  const html = await renderRoute('/')
  expect(html).not.toContain(HIDDEN_AMOUNT)
  expect(amounts(withoutPublicPrices(html)).length).toBeGreaterThan(0)
})
