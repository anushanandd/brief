import { useCallback, useEffect, useMemo, useRef, useState } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import { draftCardRules, recheckCardRules, searchCardCatalog } from '../lib/api'
import type { BenefitDraft } from '../lib/bindings/BenefitDraft'
import type { CardDraft } from '../lib/bindings/CardDraft'
import type { CardRules } from '../lib/bindings/CardRules'
import type { CardRulesChange } from '../lib/bindings/CardRulesChange'
import type { CatalogCard } from '../lib/bindings/CatalogCard'
import type { Confidence } from '../lib/bindings/Confidence'
import type { TermCheck } from '../lib/bindings/TermCheck'
import {
  appliedChange,
  cadenceLabels,
  cadences,
  confidenceLabels,
  included,
  reviewedRules,
  savedEdits,
  savedRules,
  termFor,
  weakest,
  withDescriptor,
  type BenefitEdit,
  type DraftEdits,
} from '../lib/card-drafts'
import { localDateKey } from '../lib/dates'
import { errorMessage } from '../lib/errors'
import { formatCurrency } from '../lib/format'
import type { FinanceSettings } from '../lib/schema'
import { ExternalLink } from './external-link'
import { FilterSelect } from './filter-select'
import { Check, RefreshCw, Search, X } from './icons'
import { Button, EmptyState } from './ui'

type Save = (change: (current: FinanceSettings) => FinanceSettings) => unknown

const sourceLabels = {
  catalog: 'Catalog',
  terms: 'Issuer terms',
  history: 'Your card',
  user: 'You',
} as const

export function ConfidenceChip({ confidence }: { confidence: Confidence }) {
  const { label, detail } = confidenceLabels[confidence]
  return (
    <span className={`tier-label tier-${confidence}`} title={detail}>
      {label}
    </span>
  )
}

function TermCell({ term, children }: { term?: TermCheck; children: React.ReactNode }) {
  return (
    <td>
      <span className="card-draft-term">
        {children}
        {term ? <ConfidenceChip confidence={term.confidence} /> : null}
      </span>
    </td>
  )
}

/** The drafted benefits, each editable, with the evidence for every term one click away. */
export function DraftTable({
  draft,
  edits,
  onEdit,
}: {
  draft: CardDraft
  edits: DraftEdits
  onEdit: (id: string, edit: BenefitEdit) => void
}) {
  const rules = new Map(draft.rules.benefits.map((rule) => [rule.id, rule]))
  return (
    <table className="card-draft-table">
      <thead>
        <tr>
          <th scope="col">Keep</th>
          <th scope="col">Benefit</th>
          <th scope="col">Allowance</th>
          <th scope="col">Resets</th>
          <th scope="col">Credit posts as</th>
        </tr>
      </thead>
      <tbody>
        {draft.benefits.map((benefit) => {
          const rule = rules.get(benefit.id)
          const edit = edits[benefit.id] ?? {}
          const keep = included(benefit, edits)
          const cadence = edit.cadence ?? rule?.cadence
          return (
            <tr key={benefit.id} data-keyboard-row className={keep ? undefined : 'is-left-out'}>
              <td>
                <input
                  type="checkbox"
                  checked={keep}
                  aria-label={`Keep ${benefit.name}`}
                  onChange={(event) => onEdit(benefit.id, { include: event.target.checked })}
                />
              </td>
              <td>
                <BenefitEvidence benefit={benefit} sentences={draft.sentences} />
              </td>
              <TermCell term={termFor(benefit, 'cap') ?? termFor(benefit, 'cadence')}>
                <input
                  type="number"
                  min={0}
                  step="0.01"
                  className="card-draft-amount"
                  aria-label={`${benefit.name} allowance`}
                  value={edit.cap ?? rule?.cap ?? 0}
                  onChange={(event) => onEdit(benefit.id, { cap: Number(event.target.value) })}
                />
                {cadence ? (
                  <FilterSelect
                    label={`${benefit.name} period`}
                    value={cadence}
                    options={cadences.map((value) => ({ value, label: cadenceLabels[value] }))}
                    onValueChange={(value) => {
                      const chosen = cadences.find((option) => option === value)
                      if (chosen) onEdit(benefit.id, { cadence: chosen })
                    }}
                  />
                ) : null}
              </TermCell>
              <TermCell term={termFor(benefit, 'alignment')}>
                {rule?.alignment === 'anniversary' ? 'Cardmember year' : 'Calendar'}
              </TermCell>
              <TermCell term={termFor(benefit, 'credit') ?? termFor(benefit, 'merchants')}>
                <input
                  className="card-draft-pattern"
                  aria-label={`${benefit.name} credit wording`}
                  value={edit.creditMatches ?? rule?.creditMatches ?? ''}
                  onChange={(event) => onEdit(benefit.id, { creditMatches: event.target.value })}
                />
              </TermCell>
            </tr>
          )
        })}
      </tbody>
    </table>
  )
}

function BenefitEvidence({ benefit, sentences }: { benefit: BenefitDraft; sentences: string[] }) {
  const level = weakest(benefit.terms)
  return (
    <details className="card-draft-evidence">
      <summary>
        <strong>{benefit.name}</strong>
        {level ? <ConfidenceChip confidence={level} /> : null}
        {benefit.creditsSeen ? (
          <small>
            {benefit.creditsSeen} {benefit.creditsSeen === 1 ? 'credit' : 'credits'} seen
          </small>
        ) : null}
      </summary>
      <ul>
        {benefit.terms.map((term) => (
          <li key={`${term.field}:${term.value}`}>
            <span>
              {term.field}: <strong>{term.value}</strong>
            </span>
            <ConfidenceChip confidence={term.confidence} />
            {term.source ? <small>{sourceLabels[term.source]}</small> : null}
            {term.sentence != null && sentences[term.sentence] ? (
              <blockquote>{sentences[term.sentence]}</blockquote>
            ) : term.quote ? (
              <blockquote>{term.quote}</blockquote>
            ) : null}
            {term.note ? <small>{term.note}</small> : null}
          </li>
        ))}
      </ul>
    </details>
  )
}

/**
 * A card's rules for review: the catalog card they start from, each benefit with its evidence,
 * and credits on the card no benefit explains. Given saved rules, it opens on their terms.
 * Saving makes the rules the user's, which the catalog never changes.
 */
export function CardDraftReview({
  accountId,
  accountName,
  canReadTerms,
  saved,
  save,
  onDone,
}: {
  accountId: string
  accountName: string
  /** Research is on and Exa is connected. */
  canReadTerms: boolean
  saved?: CardRules
  save: Save
  onDone: () => void
}) {
  // The rules as they were when the review opened; a later save doesn't restart it.
  const [opened] = useState(saved)
  const [readTerms, setReadTerms] = useState(false)
  const [catalogId, setCatalogId] = useState(opened?.catalog?.id)
  const [draft, setDraft] = useState<CardDraft>()
  const [edits, setEdits] = useState<DraftEdits>({})
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string>()
  const [searching, setSearching] = useState(false)
  useEffect(() => {
    let current = true
    setLoading(true)
    setError(undefined)
    draftCardRules({ accountId, catalogId, readTerms })
      .then((next) => {
        if (!current) return
        setDraft(next)
        setEdits(opened && next.catalog?.id === opened.catalog?.id ? savedEdits(next, opened) : {})
      })
      .catch((caught) => current && setError(errorMessage(caught)))
      .finally(() => current && setLoading(false))
    return () => {
      current = false
    }
  }, [accountId, catalogId, readTerms, opened])
  const edit = useCallback(
    (id: string, change: BenefitEdit) =>
      setEdits((current) => ({ ...current, [id]: { ...current[id], ...change } })),
    [],
  )
  const approve = useCallback(async () => {
    if (!draft) return
    try {
      await save((current) => savedRules(current, reviewedRules(draft, edits, opened), accountId))
      onDone()
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }, [accountId, draft, edits, onDone, save, opened])
  const latest = useRef(approve)
  latest.current = approve
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'cardRules.draft.save',
          title: 'Save the reviewed card rules',
          search: 'card rules review save benefits',
          group: 'Here',
          scope: 'page',
          disabled: !draft,
          run: () => void latest.current(),
        },
        {
          id: 'cardRules.draft.cancel',
          title: 'Close the card rules draft',
          search: 'card rules draft cancel close',
          group: 'Here',
          scope: 'page',
          run: onDone,
        },
      ],
      [draft, onDone],
    ),
  )
  return (
    <section className="card-draft" aria-label={`Rules for ${accountName}`}>
      <header className="card-draft-header">
        <h3>Rules for {accountName}</h3>
        <Button size="compact" variant="ghost" icon={X} onClick={onDone}>
          Close
        </Button>
      </header>
      <p className="card-draft-privacy">
        Brief downloads the whole open card benefits catalog from GitHub, so the request names no
        card, and checks the draft against this card's credits on this Mac.
      </p>
      {draft?.catalog ? (
        <p className="card-draft-match">
          From the catalog: <strong>{draft.catalog.name}</strong> · {draft.catalog.issuer}
          {draft.catalog.annualFee != null
            ? ` · ${formatCurrency(draft.catalog.annualFee)} fee`
            : ''}
          <Button size="compact" variant="ghost" onClick={() => setSearching(true)}>
            Not this card
          </Button>
        </p>
      ) : draft ? (
        <p className="card-draft-match">
          The catalog has no match.{' '}
          <Button size="compact" variant="ghost" onClick={() => setSearching(true)}>
            Search the catalog
          </Button>
        </p>
      ) : null}
      {searching ? (
        <CatalogSearch
          initial={draft?.candidates ?? []}
          onPick={(card) => {
            setSearching(false)
            setCatalogId(card.id)
          }}
          onCancel={() => setSearching(false)}
        />
      ) : null}
      {canReadTerms ? (
        <label className="settings-inline-check">
          <input
            type="checkbox"
            checked={readTerms}
            onChange={(event) => setReadTerms(event.target.checked)}
          />
          Also read the issuer’s terms page. Only the card’s public name and the page’s address
          leave the Mac.
        </label>
      ) : null}
      {loading ? <EmptyState>Drafting…</EmptyState> : null}
      {draft && !loading ? (
        <>
          {draft.warnings.map((warning) => (
            <p key={warning} className="transaction-edit-note">
              {warning}
            </p>
          ))}
          {draft.benefits.length ? (
            <DraftTable draft={draft} edits={edits} onEdit={edit} />
          ) : (
            <EmptyState>No benefits to draft for this card.</EmptyState>
          )}
          <UnmatchedCredits draft={draft} edits={edits} onEdit={edit} />
          {draft.termsUrl ? (
            <p className="card-draft-match">
              Checked against <ExternalLink href={draft.termsUrl}>the issuer’s terms</ExternalLink>
            </p>
          ) : null}
          <div className="chapter-actions">
            <Button
              size="compact"
              variant="primary"
              icon={Check}
              data-command="cardRules.draft.save"
              onClick={() => void approve()}
            >
              Save rules
            </Button>
          </div>
        </>
      ) : null}
      {error ? (
        <p role="alert" className="transaction-edit-note">
          {error}
        </p>
      ) : null}
    </section>
  )
}

/** Credits on the card no drafted benefit explains, each addable to a benefit's wording. */
export function UnmatchedCredits({
  draft,
  edits,
  onEdit,
}: {
  draft: CardDraft
  edits: DraftEdits
  onEdit: (id: string, edit: BenefitEdit) => void
}) {
  if (!draft.unmatchedCredits.length) return null
  const rules = new Map(draft.rules.benefits.map((rule) => [rule.id, rule]))
  const kept = draft.benefits.filter((benefit) => included(benefit, edits))
  return (
    <section className="card-draft-unmatched">
      <h3>Credits no benefit explains</h3>
      <ul>
        {draft.unmatchedCredits.map((descriptor) => (
          <li key={descriptor} data-keyboard-row>
            <code>{descriptor}</code>
            {kept.length ? (
              <FilterSelect
                label={`Add ${descriptor} to a benefit`}
                value=""
                options={[
                  { value: '', label: 'Add to…' },
                  ...kept.map(({ id, name }) => ({ value: id, label: `Add to ${name}` })),
                ]}
                onValueChange={(id) => {
                  if (!id) return
                  const current = edits[id]?.creditMatches ?? rules.get(id)?.creditMatches
                  onEdit(id, { creditMatches: withDescriptor(current, descriptor) })
                }}
              />
            ) : null}
          </li>
        ))}
      </ul>
    </section>
  )
}

function CatalogSearch({
  initial,
  onPick,
  onCancel,
}: {
  initial: CatalogCard[]
  onPick: (card: CatalogCard) => void
  onCancel: () => void
}) {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState(initial)
  const [error, setError] = useState<string>()
  const run = async () => {
    setError(undefined)
    try {
      setResults(await searchCardCatalog(query))
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }
  return (
    <form
      className="card-draft-search"
      onSubmit={(event) => {
        event.preventDefault()
        void run()
      }}
    >
      <input
        type="search"
        aria-label="Search the card catalog"
        placeholder="Card or issuer name"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
      />
      <Button type="submit" size="compact" icon={Search}>
        Search
      </Button>
      <Button type="button" size="compact" variant="ghost" onClick={onCancel}>
        Cancel
      </Button>
      <ul>
        {results.map((card) => (
          <li key={card.id} data-keyboard-row>
            <span className="chapters-row-copy">
              <strong>{card.name}</strong>
              <small>
                {card.issuer} · {card.benefits} {card.benefits === 1 ? 'benefit' : 'benefits'}
              </small>
            </span>
            <Button type="button" size="compact" onClick={() => onPick(card)}>
              Use this card
            </Button>
          </li>
        ))}
      </ul>
      {error ? (
        <p role="alert" className="transaction-edit-note">
          {error}
        </p>
      ) : null}
    </form>
  )
}

/** One change between saved rules and a fresh draft, before and after. */
export function ChangeList({ change }: { change: CardRulesChange }) {
  return (
    <ul className="card-draft-changes">
      {change.changes.map((item) => (
        <li key={`${item.benefit}:${item.field}`}>
          <strong>{item.benefit}</strong> {item.field}: <del>{item.before ?? 'none'}</del> →{' '}
          <ins>{item.after ?? 'none'}</ins>
        </li>
      ))}
    </ul>
  )
}

/**
 * The user's rules whose catalog entry or terms page changed since they saved them. Applying a
 * change keeps the old allowance as the terms before today.
 */
export function CardRulesChanges({ save }: { save: Save }) {
  const [changes, setChanges] = useState<CardRulesChange[]>()
  const [checking, setChecking] = useState(false)
  const [error, setError] = useState<string>()
  const check = useCallback(async () => {
    setChecking(true)
    setError(undefined)
    try {
      setChanges(await recheckCardRules())
    } catch (caught) {
      setError(errorMessage(caught))
    } finally {
      setChecking(false)
    }
  }, [])
  const apply = async (change: CardRulesChange) => {
    const today = localDateKey(Date.now() / 1000)
    try {
      await save((current) => {
        const saved = (current.cardRules ?? []).find(({ product }) => product === change.product)
        const next = saved ? appliedChange(saved, change.draft.rules, today) : change.draft.rules
        return {
          ...current,
          cardRules: [
            ...(current.cardRules ?? []).filter(({ product }) => product !== change.product),
            next,
          ],
        }
      })
      setChanges((current) => current?.filter((item) => item !== change))
    } catch (caught) {
      setError(errorMessage(caught))
    }
  }
  const latest = useRef(check)
  latest.current = check
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'cardRules.recheck',
          title: 'Check card rules for changes',
          search: 'card rules benefits changes terms catalog recheck',
          group: 'Here',
          scope: 'page',
          run: () => void latest.current(),
        },
      ],
      [],
    ),
  )
  return (
    <div className="card-draft-recheck">
      <Button
        size="compact"
        icon={RefreshCw}
        data-command="cardRules.recheck"
        disabled={checking}
        onClick={() => void check()}
      >
        {checking ? 'Checking…' : 'Check for changes'}
      </Button>
      {changes && !changes.length ? (
        <p className="transaction-edit-note">None of your card rules have changed.</p>
      ) : null}
      {changes?.map((change) => (
        <section key={change.product} className="card-draft-change">
          <p>
            <strong>{change.name}</strong> · {change.reason}
          </p>
          <ChangeList change={change} />
          <div className="chapter-actions">
            <Button size="compact" variant="primary" onClick={() => void apply(change)}>
              Apply
            </Button>
            <Button
              size="compact"
              variant="ghost"
              onClick={() => setChanges((current) => current?.filter((item) => item !== change))}
            >
              Dismiss
            </Button>
          </div>
        </section>
      ))}
      {error ? (
        <p role="alert" className="transaction-edit-note">
          {error}
        </p>
      ) : null}
    </div>
  )
}
