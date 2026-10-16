import { useMemo, useRef, useState, type Ref } from 'react'

import { useLatest } from '../hooks/use-latest'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { completeActivityQuery, type QuerySuggestion } from '../lib/activity-query'
import { acceptAutofill, AutofillField, autofillCompletion } from './autofill'
import { Bookmark, Search, Sparkles, X } from './icons'

export type QueryChip = { id: string; label: string; value: string; remove: () => void }

/**
 * One field for search and filters. Committed filters sit inside it as chips; the draft previews
 * live in the results, the best completion shows as ghost text, and Enter turns it into chips.
 */
export function ActivityQueryBar({
  inputRef,
  label,
  draft,
  onDraftChange,
  chips,
  suggestions,
  preview,
  onCommit,
  canSave,
  onSave,
}: {
  inputRef: Ref<HTMLInputElement>
  /** Names the field and its committed filters, e.g. "Filters, 2 active". */
  label: string
  draft: string
  onDraftChange: (value: string) => void
  chips: QueryChip[]
  /** Completions for what is being typed; the first is offered as ghost text. */
  suggestions: QuerySuggestion[]
  /** What Enter will apply, in words; empty when the draft is plain text. */
  preview: string[]
  onCommit: () => void
  canSave: boolean
  onSave: () => void
}) {
  const fieldRef = useRef<HTMLInputElement | null>(null)
  // Each chip's removal is a command too, so the palette reaches every filter the pointer can.
  // The chips are rebuilt on every render, so registration follows what they say, not their
  // identity, and each command removes the chip as it is now.
  const latestChips = useLatest(chips)
  const chipKey = chips
    .map((chip) => `${chip.id}\u0000${chip.label}\u0000${chip.value}`)
    .join('\u0001')
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        chipKey
          ? chipKey.split('\u0001').map((entry) => {
              const [id, chipLabel, value] = entry.split('\u0000')
              return {
                id: `activity.filter.remove.${id}`,
                title: `Remove ${chipLabel.toLowerCase()} filter: ${value}`,
                search: `clear filter ${chipLabel} ${value}`,
                group: 'Here',
                scope: 'page' as const,
                run: () => latestChips.current.find((chip) => chip.id === id)?.remove(),
              }
            })
          : [],
      [chipKey, latestChips],
    ),
  )
  const [focused, setFocused] = useState(false)
  const target = focused && draft.trim() ? suggestions[0] : undefined
  const completion = target
    ? autofillCompletion(draft, [completeActivityQuery(draft, target).trimEnd()])
    : undefined

  return (
    <div className="query-bar" data-focused={focused || undefined}>
      <div className="query-bar-field" role="group" aria-label={label}>
        <Search size={16} aria-hidden="true" />
        {chips.map((chip) => (
          <button
            type="button"
            className="query-chip"
            key={chip.id}
            aria-label={`Remove ${chip.label.toLowerCase()} filter: ${chip.value}`}
            data-command={`activity.filter.remove.${chip.id}`}
            onClick={chip.remove}
          >
            <span>{chip.label}</span>
            {chip.value}
            <X size={12} aria-hidden="true" />
          </button>
        ))}
        <AutofillField query={draft} completion={completion} className="query-bar-input">
          <input
            ref={(element) => {
              fieldRef.current = element
              if (typeof inputRef === 'function') inputRef(element)
              else if (inputRef) inputRef.current = element
            }}
            type="text"
            aria-label="Search and filter activity"
            aria-autocomplete={completion ? 'inline' : undefined}
            aria-keyshortcuts="/"
            // WebKit's own autofill, correction, and capitalization fight the ghost text.
            autoComplete="off"
            autoCorrect="off"
            autoCapitalize="off"
            spellCheck={false}
            value={draft}
            placeholder={chips.length ? 'Add more…' : 'Try “dining over $50 from 8/1 to 9/2”'}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            onChange={(event) => onDraftChange(event.target.value)}
            onKeyDown={(event) => {
              if (acceptAutofill(event, completion, onDraftChange)) return
              if (event.key === 'Enter') {
                event.preventDefault()
                onCommit()
              } else if (event.key === 'Backspace' && !draft && chips.length) {
                event.preventDefault()
                chips.at(-1)!.remove()
              }
            }}
          />
        </AutofillField>
        <button
          type="button"
          className="query-bar-action"
          aria-label="Save this view"
          title="Save this view"
          data-command="activity.views.save"
          disabled={!canSave}
          onClick={onSave}
        >
          <Bookmark size={16} aria-hidden="true" />
        </button>
      </div>
      {preview.length ? (
        <p className="query-preview" role="status">
          <Sparkles size={12} aria-hidden="true" />
          {preview.join(' · ')}
          <kbd>↵</kbd>
        </p>
      ) : null}
    </div>
  )
}
