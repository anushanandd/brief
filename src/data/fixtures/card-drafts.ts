import type { BenefitDraft } from '../../lib/bindings/BenefitDraft'
import type { CardDraft } from '../../lib/bindings/CardDraft'
import type { CardRule } from '../../lib/bindings/CardRule'
import type { CardRules } from '../../lib/bindings/CardRules'
import type { TermCheck } from '../../lib/bindings/TermCheck'

/** An invented card's benefit rule. */
export function cardRule(id: string, fields: Partial<CardRule> = {}): CardRule {
  return {
    id,
    name: id,
    limit: '$10 / month',
    cap: 10,
    cadence: 'monthly',
    matches: id,
    creditMatches: `${id}.*credit`,
    excludes: null,
    merchantAmountMatches: [],
    excludeMerchantAmounts: [],
    matchRequiresBefore: null,
    instruction: 'Pay with the card.',
    effectiveFrom: '2025-01-01',
    effectiveUntil: null,
    url: 'https://example.com/terms',
    logoDomain: null,
    category: null,
    timeZone: null,
    deadlineNote: null,
    allowanceChanges: [],
    issuerCredits: true,
    trackRemaining: true,
    monthCaps: {},
    estimateFromPurchases: false,
    postingDelayDays: 56,
    typicalDelayDays: 7,
    recurring: false,
    minimumCharge: null,
    renewalTerms: [],
    alignment: 'calendar',
    sources: [],
    ...fields,
  }
}

export function cardRules(benefits: CardRule[]): CardRules {
  return {
    product: 'test-rewards',
    name: 'Test Rewards Card',
    version: '2026-01-01',
    termsReviewed: 'Jan 1, 2026',
    annualFee: 95,
    annualFeeMatches: 'annual fee',
    officialNames: 'test rewards',
    officialNameExcludes: null,
    issuerCreditMatches: null,
    benefits,
    catalog: { id: 'test-rewards', hash: 'abc' },
    termsUrl: null,
    termsHash: null,
    origin: 'catalog',
    applied: '2026-01-01',
  }
}

export const termCheck = (
  field: string,
  confidence: TermCheck['confidence'],
  fields: Partial<TermCheck> = {},
): TermCheck => ({
  field,
  value: '',
  confidence,
  source: 'catalog',
  sentence: null,
  quote: null,
  note: null,
  ...fields,
})

export const benefitDraft = (id: string, terms: TermCheck[], include = true): BenefitDraft => ({
  id,
  name: id,
  terms,
  creditsSeen: 0,
  include,
})

export function cardDraft(
  benefits: BenefitDraft[],
  rules = cardRules([cardRule('streaming'), cardRule('dining')]),
): CardDraft {
  return {
    accountId: 'plaid:card',
    catalog: {
      id: 'test-rewards',
      name: 'Test Rewards Card',
      issuer: 'Test Bank',
      annualFee: 95,
      benefits: 2,
    },
    candidates: [],
    rules,
    benefits,
    termsUrl: null,
    sentences: ['You get up to $10 each month in streaming credits.'],
    unmatchedCredits: ['TEST BANK STREAMING CREDIT'],
    warnings: [],
  }
}
