import { useRef, useState } from 'react'

import { useContactPhotos } from '../hooks/use-contact-photos'
import { useFinanceEdits } from '../hooks/use-finance-edits'
import type { TransactionAnnotationChanges } from '../lib/annotations'
import { formatActivityName, formatCurrency } from '../lib/format'
import type { FinanceSnapshot, Transaction } from '../lib/schema'
import { formatActivityDate } from '../lib/spending'
import { evenSplit, splitDraftState, type SplitDraft } from '../lib/splits'
import { categoryChoices, purchasePaybacks, repayablePurchases } from '../lib/transaction-detail'
import { FilterSelect } from './filter-select'
import { Plus, X } from './icons'
import { PeoplePicker } from './people-picker'
import { Button } from './ui'

/** A purchase as a refund names it. */
const purchaseLabel = (item: Transaction) =>
  `${formatActivityName(item.merchant)} on ${formatActivityDate(item.postedOn ?? item.date)}`

/** Money in as a purchase's paybacks name it. */
const paybackLabel = (item: Transaction) =>
  `${formatActivityName(item.merchant)} on ${formatActivityDate(item.postedOn ?? item.date)} · ${formatCurrency(item.amount)}`

type Annotation = 'tag' | 'people' | 'note' | 'link'

const focus = (input: { current: HTMLElement | null }) => () => input.current?.focus()

/**
 * Edits to one transaction: its category or split between categories, whether it's someone
 * else's money, the purchase a refund or payback repays (or a purchase's paybacks), then tags,
 * people, and a note, each shown once it holds something or is being added. Each saves through
 * the native annotation command and reports its own pending and error state; Rust validates
 * and applies them.
 */
export function TransactionEdits({
  transaction,
  data,
  categoryNote,
}: {
  transaction: Transaction
  data: FinanceSnapshot
  /** Where the current category came from: the bank, a rule, or a manual choice. */
  categoryNote?: string
}) {
  const { annotate } = useFinanceEdits()
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string>()
  const [tag, setTag] = useState('')
  const [note, setNote] = useState<string>()
  const [draft, setDraft] = useState<SplitDraft[]>()
  const [adding, setAdding] = useState<ReadonlySet<Annotation>>(new Set())
  const section = useRef<HTMLElement>(null)
  const tagInput = useRef<HTMLInputElement>(null)
  const peopleInput = useRef<HTMLInputElement>(null)
  const noteInput = useRef<HTMLTextAreaElement>(null)
  const openLink = () =>
    section.current?.querySelector<HTMLElement>('.transaction-repays-select')?.click()
  // Shows the field, then reaches it once it has rendered.
  const reveal = (field: Annotation, reach: () => void) => {
    setAdding((current) => new Set(current).add(field))
    requestAnimationFrame(reach)
  }
  const people = data.settings?.people ?? []
  const photos = useContactPhotos(people)
  const savedNote = transaction.note ?? ''
  const commitNote = () => {
    if (note === undefined) return
    const next = note.trim()
    if (next === savedNote) return setNote(undefined)
    // The draft stays until the note is saved, so a failed save keeps what was typed.
    void save({ note: next }).then((saved) => {
      if (saved) setNote(undefined)
    })
  }
  const run = async (action: () => Promise<unknown>) => {
    if (pending) return false
    setPending(true)
    setError(undefined)
    try {
      await action()
      return true
    } catch (failure) {
      setError(failure instanceof Error ? failure.message : 'The edit was not saved')
      return false
    } finally {
      setPending(false)
    }
  }
  const save = (change: TransactionAnnotationChanges[string]) =>
    run(() => annotate({ [transaction.id]: change }))
  const tags = transaction.tags ?? []
  const choices = categoryChoices(data, transaction.category).map((category) => ({
    value: category,
    label: category,
  }))
  const split = draft ? splitDraftState(transaction.amount, draft) : undefined
  const purchase = transaction.refundOf
    ? data.transactions.find(({ id }) => id === transaction.refundOf)
    : undefined
  const suggested = transaction.refundOf
    ? undefined
    : data.transactions.find(
        ({ id }) =>
          id ===
          data.insights?.refundSuggestions.find(({ refundId }) => refundId === transaction.id)
            ?.purchaseId,
      )
  const repayable = transaction.refundOf || suggested ? [] : repayablePurchases(data, transaction)
  const paybacks = transaction.amount < 0 ? purchasePaybacks(data, transaction) : undefined
  const repaid = paybacks?.linked.reduce((sum, { amount }) => sum + amount, 0) ?? 0
  const startSplit = () =>
    setDraft(
      transaction.splits?.length
        ? transaction.splits.map((part) => ({
            category: part.category,
            amount: Math.abs(part.amount).toFixed(2),
          }))
        : [
            { category: transaction.category, amount: Math.abs(transaction.amount).toFixed(2) },
            { category: transaction.category, amount: '' },
          ],
    )
  const showTags = tags.length > 0 || adding.has('tag')
  const showPeople = Boolean(transaction.people?.length) || adding.has('people')
  const showNote = Boolean(savedNote) || note !== undefined || adding.has('note')
  // A link that could be made, but isn't, waits in the Add line like an empty note.
  const linkable = repayable.length > 0 || Boolean(paybacks?.candidates.length)
  const showRepays =
    transaction.amount > 0 &&
    Boolean(suggested || transaction.refundOf || (repayable.length && adding.has('link')))
  const showPaidBack = Boolean(
    paybacks?.linked.length || (paybacks?.candidates.length && adding.has('link')),
  )
  const addLink = linkable && !showRepays && !showPaidBack
  return (
    <section
      ref={section}
      className="transaction-edits"
      aria-label="Edit"
      aria-busy={pending || undefined}
    >
      {/* Targets for the inspector's C, T, P, N, and B keys; S presses the split button itself. */}
      <button
        type="button"
        hidden
        data-inspector-key="category"
        onClick={(event) =>
          event.currentTarget.parentElement
            ?.querySelector<HTMLElement>('.transaction-category-select')
            ?.click()
        }
      />
      <button
        type="button"
        hidden
        data-inspector-key="tag"
        onClick={() => reveal('tag', focus(tagInput))}
      />
      <button
        type="button"
        hidden
        data-inspector-key="people"
        onClick={() => reveal('people', focus(peopleInput))}
      />
      <button
        type="button"
        hidden
        data-inspector-key="note"
        onClick={() => reveal('note', focus(noteInput))}
      />
      {linkable ? (
        <button
          type="button"
          hidden
          data-inspector-key="repays"
          onClick={() => reveal('link', openLink)}
        />
      ) : null}
      <div className="transaction-edit-row">
        <span>Category</span>
        <div>
          {transaction.splits?.length ? (
            <>
              {transaction.splits.map((part) => (
                <span className="transaction-split-saved" key={`${part.category}:${part.amount}`}>
                  {part.category}
                  <strong className="sensitive-value">{formatCurrency(part.amount)}</strong>
                </span>
              ))}
              {draft ? null : (
                <>
                  <Button
                    size="compact"
                    variant="ghost"
                    data-inspector-key="split"
                    aria-keyshortcuts="S"
                    disabled={pending}
                    onClick={startSplit}
                  >
                    Edit split
                  </Button>
                  <Button
                    size="compact"
                    variant="ghost"
                    disabled={pending}
                    onClick={() => void save({ splits: [] })}
                  >
                    Remove split
                  </Button>
                </>
              )}
            </>
          ) : (
            <FilterSelect
              className="transaction-category-select"
              label="Category"
              value={transaction.category}
              options={choices}
              disabled={pending}
              onValueChange={(category) => {
                if (category !== transaction.category) void save({ category })
              }}
            />
          )}
          {transaction.categorySource === 'manual' && !transaction.splits?.length ? (
            <Button
              size="compact"
              variant="ghost"
              disabled={pending}
              onClick={() => void save({ category: '' })}
            >
              Reset
            </Button>
          ) : null}
          {transaction.splits?.length || draft ? null : (
            <Button
              size="compact"
              variant="ghost"
              data-inspector-key="split"
              aria-keyshortcuts="S"
              disabled={pending || transaction.amount === 0}
              onClick={startSplit}
            >
              Split
            </Button>
          )}
          {categoryNote && !transaction.splits?.length ? (
            <span className="transaction-edit-note">{categoryNote}</span>
          ) : null}
        </div>
      </div>

      {draft && split ? (
        <div className="transaction-edit-row">
          <span>Split</span>
          <div className="transaction-split">
            {draft.map((part, index) => (
              <div className="transaction-split-part" key={index}>
                <FilterSelect
                  label={`Category for part ${index + 1}`}
                  value={part.category}
                  options={choices}
                  disabled={pending}
                  onValueChange={(category) =>
                    setDraft(draft.map((item, at) => (at === index ? { ...item, category } : item)))
                  }
                />
                <input
                  className="transaction-tag-input transaction-split-amount sensitive-value"
                  aria-label={`Amount for part ${index + 1}`}
                  inputMode="decimal"
                  placeholder="0.00"
                  value={part.amount}
                  disabled={pending}
                  onChange={(event) =>
                    setDraft(
                      draft.map((item, at) =>
                        at === index ? { ...item, amount: event.target.value } : item,
                      ),
                    )
                  }
                />
                {draft.length > 2 ? (
                  <Button
                    icon={X}
                    size="icon-compact"
                    variant="ghost"
                    aria-label={`Remove part ${index + 1}`}
                    onClick={() => setDraft(draft.filter((_, at) => at !== index))}
                  />
                ) : null}
              </div>
            ))}
            <p className="transaction-edit-note" role="status">
              {split.remaining === 0 ? (
                'Parts add up to the amount.'
              ) : (
                <>
                  <span className="sensitive-value">
                    {formatCurrency(Math.abs(split.remaining))}
                  </span>{' '}
                  {split.remaining > 0 ? 'left to assign' : 'over the amount'}
                </>
              )}
            </p>
            <div className="transaction-split-actions">
              <Button
                size="compact"
                variant="ghost"
                disabled={pending || draft.length >= 12}
                onClick={() => setDraft([...draft, { category: transaction.category, amount: '' }])}
              >
                Add part
              </Button>
              <Button
                size="compact"
                variant="ghost"
                disabled={pending}
                onClick={() => {
                  const amounts = evenSplit(transaction.amount, draft.length)
                  // One amount per part; a part without one keeps what it holds.
                  setDraft(
                    draft.map((item, index) => ({
                      ...item,
                      amount: amounts[index] ?? item.amount,
                    })),
                  )
                }}
              >
                Split evenly
              </Button>
              <Button
                size="compact"
                variant="ghost"
                disabled={pending}
                onClick={() => setDraft(undefined)}
              >
                Cancel
              </Button>
              <Button
                size="compact"
                variant="positive"
                disabled={pending || !split.valid}
                onClick={() =>
                  void save({ splits: split.splits }).then((saved) => {
                    if (saved) setDraft(undefined)
                  })
                }
              >
                {pending ? 'Saving…' : 'Save split'}
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      <div className="transaction-edit-row">
        <span>For</span>
        <div>
          <Button
            size="compact"
            variant={transaction.other ? 'secondary' : 'ghost'}
            data-inspector-key="other"
            aria-keyshortcuts="F"
            aria-pressed={Boolean(transaction.other)}
            aria-label="Someone else's"
            disabled={pending}
            onClick={() => void save({ other: !transaction.other })}
          >
            {transaction.other ? 'Other' : 'Me'}
          </Button>
          {transaction.other ? (
            <span className="transaction-edit-note">
              {transaction.amount < 0 ? 'Not your spending' : 'Not income'}
            </span>
          ) : null}
        </div>
      </div>

      {showRepays ? (
        <div className="transaction-edit-row">
          <span>Repays</span>
          <div>
            {transaction.refundOf ? (
              <>
                <span>
                  {purchase ? purchaseLabel(purchase) : 'A purchase not in saved activity'}
                </span>
                <Button
                  size="compact"
                  variant="ghost"
                  disabled={pending}
                  onClick={() => void save({ refundOf: '' })}
                >
                  Unlink
                </Button>
              </>
            ) : suggested ? (
              <>
                <span className="transaction-edit-note">
                  Looks like a refund of {purchaseLabel(suggested)}
                </span>
                <Button
                  size="compact"
                  disabled={pending}
                  onClick={() => void save({ refundOf: suggested.id })}
                >
                  Link refund
                </Button>
              </>
            ) : (
              <FilterSelect
                className="transaction-repays-select"
                label="Purchase this repays"
                value=""
                options={[
                  { value: '', label: 'No purchase' },
                  ...repayable.map((item) => ({
                    value: item.id,
                    label: `${purchaseLabel(item)} · ${formatCurrency(item.amount)}`,
                  })),
                ]}
                disabled={pending}
                onValueChange={(id) => {
                  if (id) void save({ refundOf: id })
                }}
              />
            )}
          </div>
        </div>
      ) : null}

      {paybacks && showPaidBack ? (
        <div className="transaction-edit-row">
          <span>Paid back</span>
          <div className="transaction-paybacks">
            {paybacks.linked.map((payback) => (
              <span className="transaction-payback" key={payback.id}>
                <span className="sensitive-value">{paybackLabel(payback)}</span>
                <Button
                  size="compact"
                  variant="ghost"
                  disabled={pending}
                  aria-label={`Unlink ${paybackLabel(payback)}`}
                  onClick={() => void run(() => annotate({ [payback.id]: { refundOf: '' } }))}
                >
                  Unlink
                </Button>
              </span>
            ))}
            {paybacks.linked.length ? (
              <span className="transaction-edit-note">
                Your share{' '}
                <span className="sensitive-value">
                  {formatCurrency(Math.min(0, transaction.amount + repaid))}
                </span>
              </span>
            ) : null}
            {paybacks.candidates.length ? (
              <FilterSelect
                className="transaction-repays-select"
                label="Link a payback"
                value=""
                options={[
                  { value: '', label: 'Link a payback' },
                  ...paybacks.candidates.map((item) => ({
                    value: item.id,
                    label: paybackLabel(item),
                  })),
                ]}
                disabled={pending}
                onValueChange={(id) => {
                  if (id) void run(() => annotate({ [id]: { refundOf: transaction.id } }))
                }}
              />
            ) : null}
          </div>
        </div>
      ) : null}

      {showTags ? (
        <div className="transaction-edit-row">
          <span>Tags</span>
          <div>
            {tags.map((item) => (
              <span className="transaction-tag" key={item}>
                {item}
                <button
                  type="button"
                  aria-label={`Remove tag ${item}`}
                  disabled={pending}
                  onClick={() => void save({ tags: tags.filter((other) => other !== item) })}
                >
                  <X size={12} aria-hidden="true" />
                </button>
              </span>
            ))}
            <form
              onSubmit={(event) => {
                event.preventDefault()
                const next = tag.trim()
                if (!next) return
                void save({ tags: [...tags, next] }).then((saved) => {
                  if (saved) setTag('')
                })
              }}
            >
              <input
                ref={tagInput}
                className="transaction-tag-input"
                aria-label="Add a tag"
                aria-keyshortcuts="T"
                placeholder="Add tag"
                maxLength={40}
                value={tag}
                disabled={pending}
                onChange={(event) => setTag(event.target.value)}
              />
            </form>
          </div>
        </div>
      ) : null}

      {showPeople ? (
        <div className="transaction-edit-row">
          <span>People</span>
          <div>
            <PeoplePicker
              label="Add a person"
              chosen={transaction.people ?? []}
              people={people}
              photos={photos}
              pending={pending}
              inputRef={peopleInput}
              onChange={(next) => annotate({ [transaction.id]: { people: next } })}
            />
            {transaction.peopleSource === 'matched' ? (
              <span className="transaction-edit-note">Matched by name</span>
            ) : null}
          </div>
        </div>
      ) : null}

      {showNote ? (
        <div className="transaction-edit-row">
          <span>Note</span>
          <div>
            <textarea
              ref={noteInput}
              className="transaction-tag-input transaction-note-input"
              aria-label="Note"
              aria-keyshortcuts="N"
              placeholder="Add a note"
              rows={note?.includes('\n') || savedNote.includes('\n') ? 3 : 1}
              maxLength={500}
              value={note ?? savedNote}
              disabled={pending}
              onChange={(event) => setNote(event.target.value)}
              onBlur={commitNote}
              onKeyDown={(event) => {
                // Return saves; Shift+Return starts a new line; Escape drops the draft.
                if (event.key === 'Enter' && !event.shiftKey) {
                  event.preventDefault()
                  commitNote()
                }
                if (event.key === 'Escape' && note !== undefined) {
                  event.stopPropagation()
                  setNote(undefined)
                }
              }}
            />
          </div>
        </div>
      ) : null}

      {showTags && showPeople && showNote && !addLink ? null : (
        <div className="transaction-edit-row transaction-edit-add">
          <span>Add</span>
          <div>
            {showTags ? null : (
              <Button
                icon={Plus}
                size="compact"
                variant="ghost"
                aria-keyshortcuts="T"
                disabled={pending}
                onClick={() => reveal('tag', focus(tagInput))}
              >
                Tag
              </Button>
            )}
            {showPeople ? null : (
              <Button
                icon={Plus}
                size="compact"
                variant="ghost"
                aria-keyshortcuts="P"
                disabled={pending}
                onClick={() => reveal('people', focus(peopleInput))}
              >
                Person
              </Button>
            )}
            {showNote ? null : (
              <Button
                icon={Plus}
                size="compact"
                variant="ghost"
                aria-keyshortcuts="N"
                disabled={pending}
                onClick={() => reveal('note', focus(noteInput))}
              >
                Note
              </Button>
            )}
            {addLink ? (
              <Button
                icon={Plus}
                size="compact"
                variant="ghost"
                aria-keyshortcuts="B"
                disabled={pending}
                onClick={() => reveal('link', openLink)}
              >
                {transaction.amount < 0 ? 'Payback' : 'Purchase it repays'}
              </Button>
            ) : null}
          </div>
        </div>
      )}

      {error ? (
        <p className="transaction-edit-error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  )
}
