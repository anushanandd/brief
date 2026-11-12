import type { BenefitDraft } from './bindings/BenefitDraft'
import type { Cadence } from './bindings/Cadence'
import type { CardDraft } from './bindings/CardDraft'
import type { CardRule } from './bindings/CardRule'
import type { CardRules } from './bindings/CardRules'
import type { Confidence } from './bindings/Confidence'
import type { TermCheck } from './bindings/TermCheck'
import type { FinanceSettings, FinanceSnapshot } from './schema'

/** Credit cards no saved rules cover, by Rust's own reading of which cards it tracks. */
export function cardsWithoutRules(data: FinanceSnapshot) {
  const covered = new Set(
    [data.insights?.benefits, ...(data.insights?.otherCardBenefits ?? [])].flatMap((card) =>
      card ? [card.accountId] : [],
    ),
  )
  return data.accounts.filter(
    ({ id, type, balanceSource }) =>
      type === 'credit' && id !== 'all' && balanceSource !== 'spendingOnly' && !covered.has(id),
  )
}

export const confidenceLabels: Record<Confidence, { label: string; detail: string }> = {
  observed: { label: 'Observed', detail: 'A credit on your card matches it' },
  explained: { label: 'Explained', detail: "A sentence of the issuer's terms states it" },
  suggested: { label: 'Suggested', detail: 'Only the catalog says so' },
  conflict: { label: 'Conflict', detail: 'The terms say something different' },
  unconfirmed: { label: 'Unconfirmed', detail: 'Nothing supports it yet' },
}

export const cadenceLabels: Record<Cadence, string> = {
  monthly: 'Monthly',
  quarterly: 'Quarterly',
  semiannual: 'Every six months',
  annual: 'Yearly',
  renewal: 'On renewal',
  purchase: 'Per purchase',
}

export const cadences: Cadence[] = [
  'monthly',
  'quarterly',
  'semiannual',
  'annual',
  'renewal',
  'purchase',
]

// Rust names each drafted term by the rule field it backs.
const fieldNames: Record<string, readonly string[]> = {
  cap: ['cap', 'limit', 'amount'],
  cadence: ['cadence', 'frequency', 'period'],
  alignment: ['alignment', 'reset'],
  merchants: ['matches', 'merchants'],
  credit: ['creditMatches', 'credit'],
}

/** A benefit's term for one column of the review, by any name Rust gives it. */
export function termFor(benefit: BenefitDraft, column: keyof typeof fieldNames) {
  const names = fieldNames[column] ?? [column]
  return benefit.terms.find(({ field }) => names.includes(field))
}

/** The weakest evidence among a benefit's terms, for its row. */
export function weakest(terms: readonly TermCheck[]): Confidence | undefined {
  const order: Confidence[] = ['conflict', 'unconfirmed', 'suggested', 'explained', 'observed']
  return order.find((level) => terms.some(({ confidence }) => confidence === level))
}

export type BenefitEdit = {
  include?: boolean
  cap?: number
  cadence?: Cadence
  matches?: string
  creditMatches?: string
}

export type DraftEdits = Record<string, BenefitEdit>

/** Whether the draft keeps a benefit, after the user's choice. */
export const included = (benefit: BenefitDraft, edits: DraftEdits) =>
  edits[benefit.id]?.include ?? benefit.include

const escapeRegex = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** A credit pattern that also matches `descriptor`, word for word. */
export function withDescriptor(pattern: string | null | undefined, descriptor: string) {
  const literal = escapeRegex(descriptor.trim().toLowerCase())
  if (!pattern) return literal
  return pattern.split('|').includes(literal) ? pattern : `${pattern}|${literal}`
}

/**
 * The rules to save: the draft's, with the user's edits applied, edited terms marked as the
 * user's, and benefits left out dropped.
 */
export function editedRules(draft: CardDraft, edits: DraftEdits): CardRules {
  const kept = new Set(
    draft.benefits.filter((benefit) => included(benefit, edits)).map(({ id }) => id),
  )
  return {
    ...draft.rules,
    benefits: draft.rules.benefits
      .filter(({ id }) => kept.has(id))
      .map((rule) => {
        const edit = edits[rule.id]
        if (!edit) return rule
        const changed = (['cap', 'cadence', 'matches', 'creditMatches'] as const).some(
          (key) => edit[key] !== undefined && edit[key] !== rule[key],
        )
        return {
          ...rule,
          ...(edit.cap !== undefined ? { cap: edit.cap } : {}),
          ...(edit.cadence ? { cadence: edit.cadence } : {}),
          ...(edit.matches !== undefined ? { matches: edit.matches } : {}),
          ...(edit.creditMatches !== undefined ? { creditMatches: edit.creditMatches } : {}),
          sources: changed
            ? [...rule.sources, { kind: 'user' as const, url: null, quote: null, retrieved: null }]
            : rule.sources,
        }
      }),
  }
}

/** Edits that open a fresh draft on saved rules' terms, leaving out what they left out. */
export function savedEdits(draft: CardDraft, saved: CardRules): DraftEdits {
  const kept = new Map(saved.benefits.map((rule) => [rule.id, rule]))
  return Object.fromEntries(
    draft.benefits.map(({ id }) => {
      const rule = kept.get(id)
      return [
        id,
        rule
          ? {
              include: true,
              cap: rule.cap,
              cadence: rule.cadence,
              matches: rule.matches,
              creditMatches: rule.creditMatches ?? undefined,
            }
          : { include: false },
      ]
    }),
  )
}

/**
 * Reviewed rules, now the user's: the draft with the edits, keeping each saved benefit's dated
 * allowance changes.
 */
export function reviewedRules(draft: CardDraft, edits: DraftEdits, saved?: CardRules): CardRules {
  const rules = editedRules(draft, edits)
  const before = new Map(saved?.benefits.map((rule) => [rule.id, rule]))
  return {
    ...rules,
    origin: 'user',
    applied: null,
    benefits: rules.benefits.map((rule) => ({
      ...rule,
      allowanceChanges: before.get(rule.id)?.allowanceChanges ?? rule.allowanceChanges,
    })),
  }
}

/**
 * Settings with `rules` saved: they replace the same product's, and the card is assigned the
 * product unless it is the spending card and this is the only product, which it already
 * follows.
 */
export function savedRules(
  settings: FinanceSettings,
  rules: CardRules,
  accountId: string,
): FinanceSettings {
  const cardRules = [
    ...(settings.cardRules ?? []).filter(({ product }) => product !== rules.product),
    rules,
  ]
  const followsOnly = settings.spendingAccountId === accountId && cardRules.length === 1
  return {
    ...settings,
    cardRules,
    ...(followsOnly
      ? {}
      : { benefitCards: { ...settings.benefitCards, [accountId]: rules.product } }),
  }
}

/**
 * A fresh draft's rules applied over saved ones. Where an allowance changed, the saved one stays
 * as the benefit's terms until today and the new one is a dated change from today, so earlier
 * periods keep the allowance they had.
 */
export function appliedChange(saved: CardRules, fresh: CardRules, today: string): CardRules {
  const before = new Map(saved.benefits.map((rule) => [rule.id, rule]))
  return {
    ...fresh,
    benefits: fresh.benefits.map((rule): CardRule => {
      const old = before.get(rule.id)
      if (!old) return rule
      const current = latestAllowance(old)
      if (current.cap === rule.cap && current.cadence === rule.cadence) {
        return { ...rule, allowanceChanges: old.allowanceChanges }
      }
      return {
        ...rule,
        cap: old.cap,
        limit: old.limit,
        cadence: old.cadence,
        allowanceChanges: [
          ...old.allowanceChanges.filter(({ from }) => from < today),
          { from: today, cap: rule.cap, limit: rule.limit, cadence: rule.cadence },
        ],
      }
    }),
  }
}

function latestAllowance(rule: CardRule) {
  const latest = rule.allowanceChanges.toSorted((a, b) => a.from.localeCompare(b.from)).at(-1)
  return latest ?? { cap: rule.cap, cadence: rule.cadence }
}
