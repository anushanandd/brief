import { expect, it } from 'vitest'

import empty from '../data/empty.json'
import { classification } from '../data/fixtures/classification'
import { benefitActivity, insights } from '../data/fixtures/insights'
import { financeSnapshotSchema, type Transaction } from './schema'
import { mapsUrl, transactionDetail } from './transaction-detail'

const transaction = (id: string, patch: Partial<Transaction> = {}): Transaction => ({
  id,
  amount: -15.49,
  merchant: 'Example streaming',
  category: 'Entertainment',
  date: '2026-09-12',
  accountId: 'card',
  account: 'Card',
  classification: classification('expense'),
  pending: false,
  ...patch,
})

it('gathers merchant history, the expected schedule, benefit evidence, and the category origin', () => {
  const data = {
    ...financeSnapshotSchema.parse(empty),
    transactions: [
      transaction('sep', { recurringId: 'series:stream', categorySource: 'manual' }),
      transaction('aug', { recurringId: 'series:stream', date: '2026-08-12' }),
      transaction('jul', {
        recurringId: 'series:stream',
        date: '2026-07-12',
        merchant: 'EXAMPLE STREAMING ',
      }),
      transaction('other', { merchant: 'Other', recurringId: 'series:other' }),
    ],
    insights: insights({
      expected: [
        {
          id: 'series:stream:2026-10-12',
          recurringId: 'series:stream',
          title: 'Example streaming',
          kind: 'subscription',
          date: '2026-10-12',
          amount: -15.49,
          amountLow: 15.49,
          amountHigh: 15.49,
          frequency: 'Monthly',
          observations: 3,
          lastSeen: '2026-09-12',
          state: 'expected',
          accountId: 'card',
          sourceTransactionId: 'sep',
        },
      ],
      benefits: {
        accountId: 'card',
        product: 'amex-platinum',
        productName: 'American Express Platinum',
        activity: [benefitActivity({ id: 'sep', benefitName: 'Digital entertainment' })],
        windows: [],
        useFirst: [],
        history: [],
        review: [],
      },
    }),
  }
  const detail = transactionDetail(data, 'spending:sep')!
  expect(detail.history.map(({ id }) => id)).toEqual(['aug', 'jul'])
  expect(detail.expected?.frequency).toBe('Monthly')
  expect(detail.benefits.map(({ benefitName }) => benefitName)).toEqual(['Digital entertainment'])
  expect(detail.categoryOrigin).toBe('manual')
  // A grouping key alone does not make a transaction recurring.
  expect(transactionDetail(data, 'spending:other')?.expected).toBeUndefined()
  expect(transactionDetail(data, 'trade:sep')).toBeUndefined()
  expect(transactionDetail(data, 'spending:missing')).toBeUndefined()
})

it('builds a Google Maps link only from the address it is given', () => {
  expect(mapsUrl('1 Market St · San Francisco, CA')).toBe(
    'https://www.google.com/maps/search/?api=1&query=1%20Market%20St%20%C2%B7%20San%20Francisco%2C%20CA',
  )
})

it('finds what Rust flagged about a transaction and opens Maps at its coordinates', () => {
  const data = {
    ...financeSnapshotSchema.parse(empty),
    transactions: [transaction('a'), transaction('b'), transaction('trip-dinner')],
    insights: insights({
      anomalies: {
        duplicates: [
          {
            id: 'duplicate:a:b',
            accountId: 'card',
            merchant: 'Example streaming',
            amount: -15.49,
            date: '2026-09-12',
            transactionIds: ['a', 'b'],
          },
        ],
        unusual: [],
      },
      trips: {
        trips: [
          {
            id: 'trip:1',
            start: '2026-09-01',
            end: '2026-09-03',
            days: [],
            perDay: 0,
            destination: 'Austin, TX',
            abroad: false,
            total: -300,
            categories: [],
            transactionIds: ['trip-dinner'],
            ongoing: false,
            flew: false,
            stops: [],
          },
        ],
        home: ['CA, US'],
      },
    }),
  }
  expect(transactionDetail(data, 'spending:b')?.duplicate?.otherId).toBe('a')
  expect(transactionDetail(data, 'spending:trip-dinner')?.trip?.destination).toBe('Austin, TX')
  expect(transactionDetail(data, 'spending:a')?.unusual).toBeUndefined()
  expect(mapsUrl('Cafe', { lat: 37.77, lon: -122.42 })).toBe(
    'https://www.google.com/maps/search/?api=1&query=37.77%2C-122.42',
  )
})
