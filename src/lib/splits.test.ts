import { expect, it } from 'vitest'

import { evenSplit, splitDraftState } from './splits'

it('accepts parts that add up in cents and saves them with the transaction sign', () => {
  const state = splitDraftState(-100, [
    { category: ' Groceries ', amount: '60.10' },
    { category: 'Gifts', amount: '$39.90' },
  ])
  expect(state.valid).toBe(true)
  expect(state.remaining).toBe(0)
  expect(state.splits).toEqual([
    { category: 'Groceries', amount: -60.1 },
    { category: 'Gifts', amount: -39.9 },
  ])
})

it('reports what is left and rejects incomplete, unnamed, or single-part drafts', () => {
  const short = splitDraftState(-100, [
    { category: 'Groceries', amount: '60' },
    { category: 'Gifts', amount: '30' },
  ])
  expect(short.valid).toBe(false)
  expect(short.remaining).toBe(10)
  expect(
    splitDraftState(-100, [
      { category: 'Groceries', amount: '60' },
      { category: '', amount: '40' },
    ]).valid,
  ).toBe(false)
  expect(
    splitDraftState(-100, [
      { category: 'Groceries', amount: '100' },
      { category: 'Gifts', amount: '' },
    ]).valid,
  ).toBe(false)
  expect(splitDraftState(-100, [{ category: 'Groceries', amount: '100' }]).valid).toBe(false)
  expect(
    splitDraftState(-100, [
      { category: 'Groceries', amount: '130' },
      { category: 'Gifts', amount: '-30' },
    ]).valid,
  ).toBe(false)
})

it('splits evenly without losing a cent', () => {
  expect(evenSplit(-100, 3)).toEqual(['33.34', '33.33', '33.33'])
  expect(evenSplit(50, 2)).toEqual(['25.00', '25.00'])
  const state = splitDraftState(
    -100,
    evenSplit(-100, 3).map((amount, index) => ({ category: `Part ${index}`, amount })),
  )
  expect(state.valid).toBe(true)
})
