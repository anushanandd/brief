import { describe, expect, it } from 'vitest'

import {
  benefitDraft,
  cardDraft,
  cardRule,
  cardRules,
  termCheck,
} from '../data/fixtures/card-drafts'
import { insights } from '../data/fixtures/insights'
import { emptySnapshot } from '../data/fixtures/records'
import {
  appliedChange,
  cardsWithoutRules,
  editedRules,
  reviewedRules,
  savedEdits,
  savedRules,
  termFor,
  weakest,
  withDescriptor,
} from './card-drafts'
import { defaultFinanceSettings } from './finance-settings'

describe('card drafts', () => {
  it('finds a term by any field name Rust gives it, and the weakest evidence', () => {
    const drafted = benefitDraft('streaming', [
      termCheck('cap', 'explained'),
      termCheck('creditMatches', 'observed'),
    ])
    expect(termFor(drafted, 'cap')?.confidence).toBe('explained')
    expect(termFor(drafted, 'credit')?.confidence).toBe('observed')
    expect(weakest(drafted.terms)).toBe('explained')
    expect(weakest([termCheck('cap', 'observed'), termCheck('cadence', 'conflict')])).toBe(
      'conflict',
    )
    expect(weakest([])).toBeUndefined()
  })

  it('saves the edited rules: left-out benefits dropped, edits applied and marked as yours', () => {
    const drafted = cardDraft([benefitDraft('streaming', []), benefitDraft('dining', [])])
    const saved = editedRules(drafted, {
      dining: { include: false },
      streaming: { cap: 12, creditMatches: 'streaming credit' },
    })
    expect(saved.benefits.map(({ id }) => id)).toEqual(['streaming'])
    expect(saved.benefits[0]).toMatchObject({ cap: 12, creditMatches: 'streaming credit' })
    expect(saved.benefits[0]?.sources.at(-1)?.kind).toBe('user')
    const kept = editedRules(
      cardDraft([benefitDraft('streaming', []), benefitDraft('dining', [], false)]),
      { dining: { include: true } },
    )
    expect(kept.benefits.map(({ id }) => id)).toEqual(['streaming', 'dining'])
  })

  it('opens a review on the saved terms and saves the result as yours', () => {
    const drafted = cardDraft([benefitDraft('streaming', []), benefitDraft('dining', [])])
    const change = {
      from: '2026-03-01',
      cap: 15,
      limit: '$15 / month',
      cadence: 'monthly' as const,
    }
    const saved = cardRules([cardRule('streaming', { cap: 12, allowanceChanges: [change] })])
    const edits = savedEdits(drafted, saved)
    expect(edits.streaming).toMatchObject({ include: true, cap: 12 })
    expect(edits.dining).toEqual({ include: false })
    const reviewed = reviewedRules(drafted, edits, saved)
    expect(reviewed).toMatchObject({ origin: 'user', applied: null })
    expect(reviewed.benefits.map(({ id }) => id)).toEqual(['streaming'])
    expect(reviewed.benefits[0]).toMatchObject({ cap: 12, allowanceChanges: [change] })
    expect(reviewedRules(drafted, {}).origin).toBe('user')
  })

  it('replaces the same product and assigns it to the card unless the spending card follows it', () => {
    const fresh = cardRules([cardRule('streaming')])
    const settings = { ...defaultFinanceSettings, cardRules: [{ ...fresh, version: 'old' }] }
    const assigned = savedRules(settings, fresh, 'plaid:other')
    expect(assigned.cardRules).toEqual([fresh])
    expect(assigned.benefitCards).toEqual({ 'plaid:other': 'test-rewards' })
    const spending = savedRules(
      { ...defaultFinanceSettings, spendingAccountId: 'plaid:card' },
      fresh,
      'plaid:card',
    )
    expect(spending.benefitCards).toBeUndefined()
    const second = savedRules(
      {
        ...defaultFinanceSettings,
        spendingAccountId: 'plaid:card',
        cardRules: [{ ...cardRules([]), product: 'another' }],
      },
      fresh,
      'plaid:card',
    )
    expect(second.benefitCards).toEqual({ 'plaid:card': 'test-rewards' })
  })

  it('keeps the old allowance before today when a change raises it', () => {
    const saved = cardRules([
      cardRule('streaming', { cap: 10, limit: '$10 / month' }),
      cardRule('dining'),
    ])
    const fresh = cardRules([
      cardRule('streaming', { cap: 15, limit: '$15 / month' }),
      cardRule('dining'),
    ])
    const applied = appliedChange(saved, fresh, '2026-11-11')
    expect(applied.benefits[0]).toMatchObject({
      cap: 10,
      limit: '$10 / month',
      allowanceChanges: [{ from: '2026-11-11', cap: 15, limit: '$15 / month', cadence: 'monthly' }],
    })
    expect(applied.benefits[1]).toMatchObject({ cap: 10, allowanceChanges: [] })
    // A change already in effect isn't added again.
    expect(appliedChange(applied, fresh, '2026-12-01').benefits[0]?.allowanceChanges).toHaveLength(
      1,
    )
  })

  it('adds a credit descriptor to a pattern once, literally', () => {
    expect(withDescriptor(null, 'TEST BANK STREAMING CREDIT')).toBe('test bank streaming credit')
    expect(withDescriptor('streaming.*credit', 'A+ CREDIT (1)')).toBe(
      'streaming.*credit|a\\+ credit \\(1\\)',
    )
    expect(withDescriptor('a', 'A')).toBe('a')
  })

  it('lists credit cards no saved rules cover, as Rust reads them', () => {
    const data = emptySnapshot()
    const base = data.accounts[0]!
    data.accounts = [
      { ...base, id: 'plaid:covered', type: 'credit' },
      { ...base, id: 'plaid:open', type: 'credit' },
      { ...base, id: 'plaid:cash', type: 'cash' },
      { ...base, id: 'wallet:card', type: 'credit', balanceSource: 'spendingOnly' },
    ]
    data.insights = insights({
      benefits: {
        accountId: 'plaid:covered',
        product: 'test-rewards',
        productName: 'Test Rewards Card',
        activity: [],
        windows: [],
        useFirst: [],
        history: [],
        years: [],
        review: [],
      },
    })
    expect(cardsWithoutRules(data).map(({ id }) => id)).toEqual(['plaid:open'])
  })
})
