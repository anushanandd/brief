import { useEffect, useMemo, useState } from 'react'

import { visibleActionTargets } from '../lib/action-system'

export function FocusHints({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState('')
  const targets = useMemo(() => visibleActionTargets(), [])
  const matching = targets.filter(({ code }) => code.startsWith(query))

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        event.stopImmediatePropagation()
        onClose()
        return
      }
      if (event.key === 'Backspace') {
        event.preventDefault()
        event.stopImmediatePropagation()
        setQuery((current) => current.slice(0, -1))
        return
      }
      if (!/^[a-z]$/i.test(event.key) || event.metaKey || event.ctrlKey || event.altKey) return
      event.preventDefault()
      event.stopImmediatePropagation()
      const nextQuery = `${query}${event.key.toLowerCase()}`
      const next = targets.filter(({ code }) => code.startsWith(nextQuery))
      if (next.length === 1 && next[0].code === nextQuery) {
        next[0].element.focus({ preventScroll: false })
        next[0].element.scrollIntoView({ block: 'nearest', inline: 'nearest' })
        onClose()
      } else if (next.length) setQuery(nextQuery)
    }
    window.addEventListener('keydown', onKeyDown, true)
    return () => window.removeEventListener('keydown', onKeyDown, true)
  }, [onClose, query, targets])

  return (
    <div className="focus-hints" role="dialog" aria-modal="false" aria-label="Jump to control">
      <div className="focus-hints-status" aria-live="polite">
        <strong>Jump to control</strong>
        <span>{query || 'Type a hint · Esc to cancel'}</span>
      </div>
      {matching.map(({ element, label, code }) => {
        const rect = element.getBoundingClientRect()
        return (
          <button
            type="button"
            className="focus-hint"
            key={code}
            style={{ left: rect.left, top: rect.top }}
            aria-label={`Focus ${label}`}
            onClick={() => {
              element.focus({ preventScroll: false })
              onClose()
            }}
          >
            <kbd>{code.slice(query.length)}</kbd>
          </button>
        )
      })}
    </div>
  )
}
