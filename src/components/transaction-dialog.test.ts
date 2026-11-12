import { expect, it } from 'vitest'

import { transaction as record } from '../data/fixtures/records'
import type { Transaction } from '../lib/schema'
import { conflictNote, evidenceDetails, inspectorKey, walletDetails } from './transaction-dialog'

const transaction = (patch: Partial<Transaction>): Transaction =>
  record({
    id: 't',
    merchant: 'Synthetic Cafe',
    category: 'Dining',
    date: '2026-10-02',
    amount: -40,
    account: 'Card',
    pending: false,
    classification: {
      kind: 'expense',
      mark: 'initial',
      spending: true,
      credit: false,
      zelle: false,
    },
    ...patch,
  })

it('names what each graded detail rests on and what disagreeing sources each say', () => {
  const disputed = transaction({
    location: { city: 'Palo Alto', region: 'CA' },
    place: {
      city: 'Menlo Park',
      region: 'CA',
      country: 'US',
      source: 'descriptor',
      precision: 'city',
    },
    quality: {
      grade: 'conflict',
      amount: { grade: 'conflict', basis: 'statementDiffers' },
      merchant: { grade: 'supported', basis: 'providerConfident' },
      category: { grade: 'confirmed', basis: 'manual' },
      location: { grade: 'conflict', basis: 'sourcesDisagree' },
      statementAmount: -48,
    },
  })
  expect(evidenceDetails(disputed)).toEqual([
    { label: 'Amount', value: 'Plaid −$40.00, statement −$48.00', conflict: true },
    { label: 'Merchant', value: 'Plaid is sure', conflict: false },
    { label: 'Category', value: 'Set by you', conflict: false },
    { label: 'Place', value: 'Bank text Menlo Park, CA, Plaid Palo Alto, CA', conflict: true },
  ])
  expect(conflictNote(disputed, 'Place')).toBe(
    'The bank’s text says Menlo Park, CA; Plaid says Palo Alto, CA. Brief shows the bank’s text.',
  )
  expect(conflictNote(disputed, 'Amount')).toContain('the statement shows −$48.00')
  expect(evidenceDetails(transaction({}))).toEqual([])
})

it('maps the inspector keys and leaves every other key to the page', () => {
  expect(inspectorKey('j')).toBe('next')
  expect(inspectorKey('ArrowDown')).toBe('next')
  expect(inspectorKey('k')).toBe('previous')
  expect(inspectorKey('ArrowUp')).toBe('previous')
  expect(inspectorKey('c')).toBe('category')
  expect(inspectorKey('t')).toBe('tag')
  expect(inspectorKey('p')).toBe('people')
  expect(inspectorKey('s')).toBe('split')
  expect(inspectorKey('f')).toBe('other')
  expect(inspectorKey('u')).toBe('unlink')
  expect(inspectorKey('o')).toBe('transfer')
  expect(inspectorKey('a')).toBe('accept')
  expect(inspectorKey('d')).toBe('reject')
  expect(inspectorKey('x')).toBeUndefined()
  expect(inspectorKey('Enter')).toBeUndefined()
})

it('keeps only Wallet’s identifiers and a name it disagrees on under Raw records', () => {
  const wallet = {
    recordId: 'w1',
    categorized: false,
    phone: '(555) 555-0100',
    terminalAmount: -30,
    network: { merchantCode: 5812, terminalId: 'T9' },
  }
  expect(walletDetails(transaction({ wallet: { ...wallet, merchant: 'Synthetic Cafe' } }))).toEqual(
    [
      { label: 'Merchant code', value: '5812', raw: true },
      { label: 'Terminal ID', value: 'T9', raw: true },
      { label: 'Wallet ID', value: 'w1', raw: true },
    ],
  )
  expect(walletDetails(transaction({ wallet: { ...wallet, merchant: 'Cafe Inc' } }))[0]).toEqual({
    label: 'Wallet’s name',
    value: 'Cafe Inc',
  })
})
