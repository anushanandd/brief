import { useEffect, useRef, type Dispatch, type SetStateAction } from 'react'

type SearchShortcutEvent = Pick<KeyboardEvent, 'altKey' | 'ctrlKey' | 'key' | 'metaKey'>

export function searchShortcutAction(
  event: SearchShortcutEvent,
  isEditing: boolean,
  isSearchFocused: boolean,
  hasQuery: boolean,
) {
  if (event.key === 'Escape' && isSearchFocused) return hasQuery ? 'clear' : 'blur'
  if (event.key === '/' && !event.metaKey && !event.ctrlKey && !event.altKey && !isEditing) {
    return 'focus'
  }
  return undefined
}

export function useSearchShortcuts(query: string, setQuery: Dispatch<SetStateAction<string>>) {
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const target = event.target
      const isEditing =
        target instanceof HTMLElement &&
        (target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName))
      const action = searchShortcutAction(
        event,
        isEditing,
        target === inputRef.current,
        query.length > 0,
      )
      if (!action) return
      event.preventDefault()
      if (action === 'focus') inputRef.current?.focus()
      else if (action === 'clear') setQuery('')
      else inputRef.current?.blur()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [query, setQuery])

  return inputRef
}
