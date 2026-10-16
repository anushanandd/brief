import type { KeyboardEvent, ReactNode } from 'react'

import { paletteCompletion } from '../lib/action-system'

/**
 * Inline autofill shared by every search field: the rest of the best match appears as ghost
 * text after the caret, and Tab accepts it a word at a time. The command palette set the pattern.
 */
export const autofillCompletion = paletteCompletion
export type AutofillCompletion = ReturnType<typeof paletteCompletion>

export function AutofillField({
  query,
  completion,
  className,
  children,
}: {
  query: string
  completion: AutofillCompletion
  className?: string
  children: ReactNode
}) {
  return (
    <span className={`autofill-field${className ? ` ${className}` : ''}`}>
      {completion ? (
        <span className="autofill-ghost" aria-hidden="true">
          <span>{query}</span>
          {completion.full.slice(query.length)}
        </span>
      ) : null}
      {children}
    </span>
  )
}

/** Tab takes the next word of the ghost text; returns whether it handled the key. */
export function acceptAutofill(
  event: Pick<KeyboardEvent, 'key' | 'shiftKey' | 'preventDefault'>,
  completion: AutofillCompletion,
  setQuery: (value: string) => void,
) {
  if (event.key !== 'Tab' || event.shiftKey || !completion) return false
  event.preventDefault()
  setQuery(completion.step)
  return true
}
