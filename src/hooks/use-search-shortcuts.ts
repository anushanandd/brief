import { useEffect, useEffectEvent, useMemo, useRef } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import { shortcutOverlayOpen } from '../lib/keyboard'

/** Escape in the search field clears a query, then leaves the field. */
export function searchEscapeAction(hasQuery: boolean) {
  return hasQuery ? 'clear' : 'blur'
}

export function useSearchShortcuts(query: string, setQuery: (value: string) => void) {
  const inputRef = useRef<HTMLInputElement>(null)
  const actions = useMemo<AppAction[]>(
    () => [
      {
        id: 'search.focus',
        title: 'Focus page search',
        search: 'find search current page',
        group: 'Here',
        scope: 'page',
        bindings: [{ sequence: ['/'], label: '/' }],
        run: () => inputRef.current?.focus(),
      },
      ...(query
        ? [
            {
              id: 'search.clear',
              title: 'Clear page search',
              search: 'reset query',
              group: 'Here',
              scope: 'page' as const,
              run: () => setQuery(''),
            },
          ]
        : []),
    ],
    [query, setQuery],
  )
  useRegisterActions(actions)

  // The key dispatcher leaves keys typed in a field to the field, so the search field handles
  // its own Escape.
  const escape = useEffectEvent(() => {
    if (searchEscapeAction(query.length > 0) === 'clear') setQuery('')
    else inputRef.current?.blur()
  })
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.target !== inputRef.current) return
      if (event.defaultPrevented || event.isComposing || shortcutOverlayOpen()) return
      event.preventDefault()
      escape()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])

  return inputRef
}
