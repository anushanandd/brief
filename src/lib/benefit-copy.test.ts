import { describe, expect, it } from 'vitest'

import { benefitActivity, benefitWindow } from '../data/fixtures/insights'
import {
  benefitNote,
  benefitStatus,
  benefitWarning,
  creditLagNote,
  purchaseCreditLabel,
} from './benefit-copy'

const purchase = (credit: NonNullable<Parameters<typeof purchaseCreditLabel>[0]>) =>
  benefitActivity({ id: `purchase-${credit.state}`, kind: 'purchase', amount: -40, credit })

describe('benefit copy', () => {
  it('labels each native purchase credit state', () => {
    expect(purchaseCreditLabel({ state: 'credited', amount: 13.99 })).toBe('Credited $13.99')
    expect(purchaseCreditLabel({ state: 'expected', amount: 9.99, by: '2026-09-13' })).toBe(
      'Credit due by Sep 13',
    )
    expect(purchaseCreditLabel({ state: 'late', amount: 9.99, by: '2026-10-27' })).toBe(
      'Late · allowed until Oct 27',
    )
    expect(purchaseCreditLabel({ state: 'not-credited' })).toBe('No credit')
    expect(purchaseCreditLabel({ state: 'unlikely' })).toBe(
      'Likely not covered: unlike every credited purchase',
    )
    expect(purchaseCreditLabel({ state: 'over-allowance' })).toBe('Over allowance')
    expect(purchaseCreditLabel(undefined)).toBeNull()
  })

  it('states credited progress and an exact remainder', () => {
    const benefit = benefitWindow({
      status: 'credited',
      creditedAmount: 35,
      remainingAmount: 15,
      remainingRange: { low: 15, high: 15 },
    })
    expect(benefitStatus(benefit)).toEqual({ text: '$35.00 of $50.00 credited' })
    expect(benefitNote(benefit)).toBe('$15.00 left')
  })

  it('shows a range when a credit may belong to the previous period', () => {
    const benefit = benefitWindow({
      status: 'credited',
      creditedAmount: 25,
      uncertainCreditAmount: 25,
      periodUncertain: true,
      remainingAmount: null,
      remainingRange: { low: 25, high: 50 },
    })
    expect(benefitNote(benefit)).toBe('$25.00–$50.00 left · $25.00 may be last period’s')
  })

  it('treats a remainder below the smallest qualifying charge as used', () => {
    const benefit = benefitWindow({
      status: 'credited',
      creditedAmount: 119.88,
      cap: 120,
      remainingAmount: 0,
      remainingRange: { low: 0, high: 0 },
      unusableRemainder: 0.12,
    })
    expect(benefitStatus(benefit)).toEqual({
      text: '$119.88 of $120.00 credited',
      tone: 'positive',
    })
    expect(benefitNote(benefit)).toBe('Used · $0.12 left over')
  })

  it('says where the history starts when it starts inside the window', () => {
    const benefit = benefitWindow({
      status: 'credited',
      creditedAmount: 25,
      historyFrom: '2026-05-09',
      remainingAmount: 25,
      remainingRange: { low: 25, high: 25 },
    })
    expect(benefitNote(benefit)).toBe('$25.00 left · history from May 9')
  })

  it('reports an expected credit before any credit posts', () => {
    const benefit = benefitWindow({
      status: 'matched',
      awaitingCreditAmount: 13.99,
      remainingRange: { low: 50, high: 50 },
    })
    expect(benefitStatus(benefit)).toEqual({ text: '$13.99 credit expected' })
    expect(benefitNote(benefit)).toBe('$50.00 left · $13.99 expected')
  })

  it('never calls a partner credit missing when its use is invisible', () => {
    const external = benefitWindow({ status: 'external', remainingRange: undefined })
    expect(benefitStatus({ ...external, estimatedCreditAmount: 0 })).toEqual({
      text: 'Use not visible on the card',
    })
    expect(benefitStatus({ ...external, estimatedCreditAmount: 10 })).toEqual({
      text: 'Likely used · $10.00',
    })
  })

  it('warns about purchases whose credit is late or never came', () => {
    expect(benefitWarning(benefitWindow({ activity: [purchase({ state: 'not-credited' })] }))).toBe(
      'No credit for Example Tables dining credit · check enrollment',
    )
    expect(
      benefitWarning(
        benefitWindow({
          activity: [
            purchase({ state: 'not-credited' }),
            purchase({ state: 'late', amount: 40, by: '2026-10-01' }),
          ],
        }),
      ),
    ).toBe('No credit for 2 purchases · check enrollment')
    expect(
      benefitWarning(
        benefitWindow({
          activity: [purchase({ state: 'expected', amount: 40, by: '2026-09-20' })],
        }),
      ),
    ).toBeNull()
  })

  it('says where the posting delay comes from', () => {
    expect(creditLagNote(benefitWindow({ creditLagDays: 5, creditLagLearned: true }))).toBe(
      'Within 5 days on this card',
    )
    expect(creditLagNote(benefitWindow())).toBe('Usually 7 days; up to 56')
  })
})
