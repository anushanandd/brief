import { expect, it } from 'vitest'

import { classification } from '../data/fixtures/classification'
import { emptySnapshot, transaction as record } from '../data/fixtures/records'
import { buildActivities } from './activity'
import type { Transaction } from './schema'
import { moneyKind, transactionMarkKind } from './transaction-kind'

it('uses the same positive Zelle income policy in Money, Overview, and Activity', () => {
  const transaction = record({
    id: 'synthetic',
    account: 'Example',
    accountId: 'example',
    merchant: 'Zelle example',
    category: 'Transfer',
    amount: 100,
    classification: classification('income', { zelle: true }),
    pending: false,
    date: '2026-09-18',
  })
  expect(transactionMarkKind(transaction)).toBe('income')
  expect(moneyKind(transaction)).toBe('income')
  const data = {
    ...emptySnapshot(),
    updatedAt: '2026-09-18T12:00:00Z',
    transactions: [transaction],
  }
  expect(buildActivities(data)[0].kind).toBe('income')
  expect(
    transactionMarkKind({
      ...transaction,
      amount: -100,
      classification: classification('transfer', { zelle: true }),
    }),
  ).toBe('transfer')
})

it.each([
  ['INTEREST', 'Income', 'interest'],
  ['Dividend · TEST', 'Income', 'dividend'],
  ['Example payroll', 'Income', 'income'],
  ['Example transfer', 'Transfer', 'transfer'],
] as const)('classifies %s consistently for marks and Money', (merchant, category, expected) => {
  const transaction = record({
    id: 'synthetic',
    merchant,
    category,
    date: '2026-09-18',
    amount: 10,
    account: 'Example',
    classification: classification(expected),
    pending: false,
  })
  expect(transactionMarkKind(transaction)).toBe(expected)
  expect(moneyKind(transaction)).toBe(expected)
})

it('treats airline fee reimbursements as credits despite a provider fee category', () => {
  const transaction = record({
    id: 'synthetic-credit',
    merchant: 'Example Airline Fee Reimbursement',
    category: 'Bank Fees',
    amount: 25,
    account: 'Example card',
    classification: classification('reimbursement'),
    pending: false,
    date: '2026-09-18',
  })
  expect(transactionMarkKind(transaction)).toBe('refund')
  expect(moneyKind(transaction)).toBe('reimbursement')
  const data = { ...emptySnapshot(), transactions: [transaction] }
  expect(buildActivities(data)[0]).toMatchObject({
    kind: 'credit',
    category: 'Credit',
    detail: 'Example card · Credit',
  })
  expect(
    moneyKind({
      ...transaction,
      merchant: 'Example annual fee',
      amount: -25,
      classification: classification('fee', { spending: true }),
    }),
  ).toBe('fee')
})

it('uses native financial meaning even when display text resembles a different kind', () => {
  const data = emptySnapshot()
  data.transactions = [
    record({
      id: 'synthetic',
      merchant: 'Transfer',
      category: 'Transfer',
      date: '2026-09-18',
      amount: 10,
      account: 'Example',
      pending: false,
      classification: classification('income'),
    }),
  ]
  expect(moneyKind(data.transactions[0])).toBe('income')
  expect(buildActivities(data)[0].kind).toBe('income')
})

const purchase = (id: string, category: string, amount: number): Transaction =>
  record({
    id,
    account: 'Card',
    accountId: 'card',
    merchant: 'Example Market',
    category,
    amount,
    classification: classification('expense'),
    pending: false,
    date: '2026-09-10',
    postedOn: '2026-09-10',
  })

it('divides a split into its parts, each keeping the transaction', async () => {
  const { splitParts } = await import('./transaction-kind')
  const split: Transaction = {
    ...purchase('split', 'Groceries', -100),
    splits: [
      { category: 'Groceries', amount: -70 },
      { category: 'Gifts', amount: -30 },
    ],
  }
  const whole = purchase('whole', 'Groceries', -20)
  expect(splitParts(whole)).toEqual([whole])
  expect(splitParts(split).map(({ id, category, amount }) => [id, category, amount])).toEqual([
    ['split', 'Groceries', -70],
    ['split', 'Gifts', -30],
  ])
})
