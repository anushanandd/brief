import { useRef, useState } from 'react'

import { useFinanceEdits } from '../hooks/use-finance-edits'
import type { TransactionAnnotationChanges } from '../lib/annotations'
import { formatActivityName, formatCurrency } from '../lib/format'
import type { FinanceSnapshot, Transaction } from '../lib/schema'
import { formatActivityDate } from '../lib/spending'
import { evenSplit, splitDraftState, type SplitDraft } from '../lib/splits'
import { categoryChoices } from '../lib/transaction-detail'
import { FilterSelect } from './filter-select'
import { X } from './icons'
import { Button } from './ui'

/**
 * Edits to one transaction: its category, whether it's someone else's money, tags, a split
 * between categories, and the purchase a refund repays. Each saves through the native annotation
 * command and reports its own pending and error state; Rust validates and applies them.
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
  const [draft, setDraft] = useState<SplitDraft[]>()
  const tagInput = useRef<HTMLInputElement>(null)
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
  const purchaseLabel = (item: Transaction) =>
    `${formatActivityName(item.merchant)} on ${formatActivityDate(item.postedOn ?? item.date, data.updatedAt)}`

  return (
    <section className="transaction-edits" aria-label="Edit" aria-busy={pending || undefined}>
      {/* Targets for the inspector's C and T keys; S presses the split button itself. */}
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
        onClick={() => tagInput.current?.focus()}
      />
      <div className="transaction-edit-row">
        <span>Category</span>
        <div>
          {transaction.splits?.length ? (
            <span className="transaction-edit-note">Set by the split below</span>
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
          {categoryNote && !transaction.splits?.length ? (
            <span className="transaction-edit-note">{categoryNote}</span>
          ) : null}
        </div>
      </div>

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

      <div className="transaction-edit-row">
        <span>Split</span>
        <div className="transaction-split">
          {draft && split ? (
            <>
              {draft.map((part, index) => (
                <div className="transaction-split-part" key={index}>
                  <FilterSelect
                    label={`Category for part ${index + 1}`}
                    value={part.category}
                    options={choices}
                    disabled={pending}
                    onValueChange={(category) =>
                      setDraft(
                        draft.map((item, at) => (at === index ? { ...item, category } : item)),
                      )
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
                  onClick={() =>
                    setDraft([...draft, { category: transaction.category, amount: '' }])
                  }
                >
                  Add part
                </Button>
                <Button
                  size="compact"
                  variant="ghost"
                  disabled={pending}
                  onClick={() => {
                    const amounts = evenSplit(transaction.amount, draft.length)
                    setDraft(draft.map((item, index) => ({ ...item, amount: amounts[index] })))
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
            </>
          ) : (
            <>
              {transaction.splits?.map((part) => (
                <span className="transaction-split-saved" key={`${part.category}:${part.amount}`}>
                  {part.category}
                  <strong className="sensitive-value">{formatCurrency(part.amount)}</strong>
                </span>
              ))}
              <Button
                size="compact"
                data-inspector-key="split"
                aria-keyshortcuts="S"
                disabled={pending || transaction.amount === 0}
                onClick={() =>
                  setDraft(
                    transaction.splits?.length
                      ? transaction.splits.map((part) => ({
                          category: part.category,
                          amount: Math.abs(part.amount).toFixed(2),
                        }))
                      : [
                          {
                            category: transaction.category,
                            amount: Math.abs(transaction.amount).toFixed(2),
                          },
                          { category: transaction.category, amount: '' },
                        ],
                  )
                }
              >
                {transaction.splits?.length ? 'Edit split' : 'Split between categories'}
              </Button>
              {transaction.splits?.length ? (
                <Button
                  size="compact"
                  variant="ghost"
                  disabled={pending}
                  onClick={() => void save({ splits: [] })}
                >
                  Remove split
                </Button>
              ) : null}
            </>
          )}
        </div>
      </div>

      {transaction.amount > 0 && (purchase || suggested || transaction.refundOf) ? (
        <div className="transaction-edit-row">
          <span>Refund</span>
          <div>
            {transaction.refundOf ? (
              <>
                <span className="transaction-edit-note">
                  {purchase
                    ? `Repays ${purchaseLabel(purchase)}`
                    : 'Linked purchase is not in saved activity'}
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
            ) : null}
          </div>
        </div>
      ) : null}

      {error ? (
        <p className="transaction-edit-error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  )
}
