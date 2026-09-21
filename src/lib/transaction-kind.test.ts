import { expect, it } from 'vitest'

import empty from '../data/empty.json'
import { classification } from '../data/fixtures/classification'
import { buildActivities } from './activity'
import { homeOverviewMetrics } from './home-overview'
import { moneyKind } from './money'
import { financeSnapshotSchema, type Transaction } from './schema'
import { transactionMarkKind } from './transaction-kind'

it('uses the same positive Zelle income policy in Money, Overview, and Activity', () => {
  const transaction: Transaction = {
    id: 'synthetic',
    account: 'Example',
    accountId: 'example',
    merchant: 'Zelle example',
    category: 'Transfer',
    amount: 100,
    classification: classification('income', { zelle: true }),
    pending: false,
    date: '2026-09-18',
  }
  expect(transactionMarkKind(transaction)).toBe('income')
  expect(moneyKind(transaction)).toBe('income')
  const data = {
    ...financeSnapshotSchema.parse(empty),
    updatedAt: '2026-09-18T12:00:00Z',
    transactions: [transaction],
  }
  expect(
    homeOverviewMetrics(data, undefined, {
      start: Date.parse('2026-09-01T00:00:00Z') / 1000,
      end: Date.parse('2026-09-18T12:00:00Z') / 1000,
    }).income,
  ).toBe(100)
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
  const transaction: Transaction = {
    id: 'synthetic',
    merchant,
    category,
    date: '2026-09-18',
    amount: 10,
    account: 'Example',
    classification: classification(expected),
    pending: false,
  }
  expect(transactionMarkKind(transaction)).toBe(expected)
  expect(moneyKind(transaction)).toBe(expected)
})

it('treats airline fee reimbursements as credits despite a provider fee category', () => {
  const transaction: Transaction = {
    id: 'synthetic-credit',
    merchant: 'Example Airline Fee Reimbursement',
    category: 'Bank Fees',
    amount: 25,
    account: 'Example card',
    classification: classification('reimbursement'),
    pending: false,
    date: '2026-09-18',
  }
  expect(transactionMarkKind(transaction)).toBe('refund')
  expect(moneyKind(transaction)).toBe('reimbursement')
  const data = { ...financeSnapshotSchema.parse(empty), transactions: [transaction] }
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
  const data = financeSnapshotSchema.parse(empty)
  data.transactions = [
    {
      id: 'synthetic',
      merchant: 'Transfer',
      category: 'Transfer',
      date: '2026-09-18',
      amount: 10,
      account: 'Example',
      pending: false,
      classification: classification('income'),
    },
  ]
  expect(moneyKind(data.transactions[0])).toBe('income')
  expect(buildActivities(data)[0].kind).toBe('income')
})
