import { useRef, useState } from 'react'

import type { ActivityItem } from '../lib/activity'
import type { FinanceSnapshot } from '../lib/schema'
import { categoryChoices } from '../lib/transaction-detail'
import { FilterSelect } from './filter-select'
import { Check, X } from './icons'
import { Keys } from './keys'
import { Button } from './ui'

/** What a bulk edit may set on every selected transaction at once. */
export type SelectionEdit =
  | { category: string }
  | { tag: string }
  | { reviewed: true }
  | { other: boolean }
  | { person: string }

/**
 * The selected rows' shared controls: a category, a tag, a person, reviewed, and who it's for,
 * each applied to every selected transaction by one native save. X selects a row; the keys
 * here work while anything is selected.
 */
export function ActivitySelectionBar({
  data,
  selected,
  pending,
  onEdit,
  onClear,
}: {
  data: FinanceSnapshot
  selected: readonly ActivityItem[]
  pending: boolean
  onEdit: (edit: SelectionEdit) => void
  onClear: () => void
}) {
  const [tag, setTag] = useState('')
  const tagInput = useRef<HTMLInputElement>(null)
  const allOther = selected.every((activity) => activity.other)
  const choices = categoryChoices(data, '').map((category) => ({
    value: category,
    label: category,
  }))
  const people = (data.settings?.people ?? [])
    .map(({ id, name }) => ({ value: id, label: name }))
    .toSorted((left, right) => left.label.localeCompare(right.label))
  return (
    <div
      className="activity-selection-bar"
      role="toolbar"
      aria-label={`${selected.length} selected`}
      aria-busy={pending || undefined}
    >
      <strong>
        {selected.length} selected
        <Keys label="X" />
      </strong>
      <span data-selection-key="category">
        <FilterSelect
          label="Category for the selection"
          command="activity.selection.category"
          value=""
          options={[{ value: '', label: 'Category…' }, ...choices]}
          disabled={pending}
          onValueChange={(category) => {
            if (category) onEdit({ category })
          }}
        />
      </span>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          const next = tag.trim()
          if (!next) return
          onEdit({ tag: next })
          setTag('')
        }}
      >
        <input
          ref={tagInput}
          className="transaction-tag-input"
          aria-label="Tag for the selection"
          aria-keyshortcuts="T"
          data-selection-key="tag"
          placeholder="Add tag"
          maxLength={40}
          value={tag}
          disabled={pending}
          onChange={(event) => setTag(event.target.value)}
        />
      </form>
      {people.length ? (
        <span data-selection-key="person">
          <FilterSelect
            label="Person for the selection"
            command="activity.selection.person"
            value=""
            options={[{ value: '', label: 'Person…' }, ...people]}
            disabled={pending}
            onValueChange={(person) => {
              if (person) onEdit({ person })
            }}
          />
        </span>
      ) : null}
      <Button
        icon={Check}
        size="compact"
        variant="ghost"
        aria-keyshortcuts="R"
        data-command="activity.selection.reviewed"
        disabled={pending}
        onClick={() => onEdit({ reviewed: true })}
      >
        Reviewed
      </Button>
      <Button
        size="compact"
        variant="ghost"
        aria-keyshortcuts="F"
        data-command="activity.selection.other"
        disabled={pending}
        onClick={() => onEdit({ other: !allOther })}
      >
        {allOther ? 'Mine' : 'Someone else’s'}
      </Button>
      <Button
        icon={X}
        size="compact"
        variant="ghost"
        aria-keyshortcuts="Escape"
        data-command="activity.selection.clear"
        disabled={pending}
        onClick={onClear}
      >
        Clear
      </Button>
    </div>
  )
}
