import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { CategoryMark } from './category-mark'

const icon = (category: string, props: Partial<Parameters<typeof CategoryMark>[0]> = {}) =>
  renderToStaticMarkup(<CategoryMark category={category} {...props} />).match(
    /<svg[^>]*>[\s\S]*?<\/svg>/,
  )?.[0]

it('gives each seeded activity category and key activity type a distinct icon', () => {
  const icons = [
    'Dining',
    'Groceries',
    'Travel',
    'Transport',
    'Shopping',
    'Utilities',
    'Entertainment',
    'Income',
    'Credit',
    'Trade',
    'Other',
    'Transfer',
    'Interest',
    'Dividend',
    'Refund',
    'Payment',
    'Cash',
    'Bank Fees',
    'Food And Drink',
  ].map((category) => icon(category))
  expect(icons.every(Boolean)).toBe(true)
  expect(new Set(icons).size).toBe(icons.length)
})

it('uses the native transaction mark and signed transfer tone in activity rows', () => {
  const transferIn = renderToStaticMarkup(
    <CategoryMark category="Other" kind="transfer" mark="transfer" amount={100} />,
  )
  const transferOut = renderToStaticMarkup(
    <CategoryMark category="Other" kind="transfer" mark="transfer" amount={-100} />,
  )
  expect(icon('Other', { kind: 'transfer', mark: 'transfer', amount: 100 })).toBe(icon('Transfer'))
  expect(transferIn).toContain('var(--positive)')
  expect(transferOut).toContain('var(--negative)')
  expect(icon('Other', { mark: 'dividend' })).toBe(icon('Dividend'))
  expect(icon('Other', { mark: 'fee' })).toBe(icon('Bank Fees'))
})

it('uses distinct category colors without treating every unfamiliar category as other', () => {
  expect(renderToStaticMarkup(<CategoryMark category="Medical" />)).toContain('var(--data-4)')
  expect(icon('Medical')).not.toBe(icon('Other'))
  expect(renderToStaticMarkup(<CategoryMark category="Unclassified" />)).toContain(
    'var(--spending-other)',
  )
})
