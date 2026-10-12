import { describe, expect, it } from 'vitest'

import { benefitActivity, benefitWindow } from '../data/fixtures/insights'
import {
  benefitNote,
  benefitStatus,
  benefitWarning,
  creditLagNote,
  creditSummaryNote,
  purchaseCreditLabel,
} from './benefit-copy'

const purchase = (credit: NonNullable<Parameters<typeof purchaseCreditLabel>[0]>) =>
  benefitActivity({ id: `purchase-${credit.state}`, kind: 'purchase', amount: -40, credit })

describe('benefit copy', () => {
  it('labels each native purchase credit state', () => {
    expect(purchaseCreditLabel({ state: 'credited', amount: 13.99 })).toBe('Credited $13.99')
    expect(purchaseCreditLabel({ state: 'expected', amount: 9.99, by: '2026-09-13' })).toBe(
      'Credit expected by Sep 13',
    )
    expect(purchaseCreditLabel({ state: 'late', amount: 9.99, by: '2026-10-27' })).toBe(
      'Credit late · allowed until Oct 27',
    )
    expect(purchaseCreditLabel({ state: 'not-credited' })).toBe('No credit posted')
    expect(purchaseCreditLabel({ state: 'over-allowance' })).toBe('Over this period’s allowance')
    expect(purchaseCreditLabel(undefined)).toBeNull()
  })

  it('states credited progress and an exact remainder', () => {
    const benefit = benefitWindow({
      status: 'credited',
      creditedAmount: 70,
      remainingAmount: 30,
      remainingRange: { low: 30, high: 30 },
    })
    expect(benefitStatus(benefit)).toEqual({ text: '$70.00 of $100.00 credited' })
    expect(benefitNote(benefit)).toBe('$30.00 left')
  })

  it('shows a range when a credit may belong to the previous period', () => {
    const benefit = benefitWindow({
      status: 'credited',
      creditedAmount: 50,
      uncertainCreditAmount: 50,
      periodUncertain: true,
      remainingAmount: null,
      remainingRange: { low: 50, high: 100 },
    })
    expect(benefitNote(benefit)).toBe(
      '$50.00–$100.00 left · $50.00 may belong to the previous period',
    )
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
    expect(benefitNote(benefit)).toBe('Used · $0.12 is below the smallest qualifying charge')
  })

  it('reports an expected credit before any credit posts', () => {
    const benefit = benefitWindow({
      status: 'matched',
      awaitingCreditAmount: 13.99,
      remainingRange: { low: 100, high: 100 },
    })
    expect(benefitStatus(benefit)).toEqual({ text: '$13.99 credit expected' })
    expect(benefitNote(benefit)).toBe('$100.00 left · $13.99 expected')
  })

  it('never calls invisible Uber Cash use missing', () => {
    const external = benefitWindow({ status: 'external', remainingRange: undefined })
    expect(benefitStatus({ ...external, estimatedCreditAmount: 0 })).toEqual({
      text: 'Use not visible on the card',
    })
    expect(benefitStatus({ ...external, estimatedCreditAmount: 15 })).toEqual({
      text: 'Likely used · $15.00',
    })
  })

  it('warns about purchases whose credit is late or never came', () => {
    expect(benefitWarning(benefitWindow({ activity: [purchase({ state: 'not-credited' })] }))).toBe(
      'No credit for Resy credit · check enrollment',
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
      'Credits for this benefit have posted within 5 days on this card.',
    )
    expect(creditLagNote(benefitWindow())).toBe(
      'Credits can take up to 56 days to post under the terms.',
    )
  })

  it('explains what the missed total leaves out', () => {
    expect(
      creditSummaryNote({
        earned: 0,
        missed: { low: 0, high: 0 },
        unclaimed: 75,
        settling: 1,
        unknown: 2,
      }),
    ).toBe(
      '1 period still settling · 2 periods not visible in the card history · $75.00 unclaimed in benefits you haven’t used',
    )
    expect(
      creditSummaryNote({
        earned: 0,
        missed: { low: 0, high: 0 },
        unclaimed: 0,
        settling: 0,
        unknown: 0,
      }),
    ).toBe('')
  })
})
