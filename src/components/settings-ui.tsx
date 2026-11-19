import { createContext, useContext, useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { ComponentPropsWithoutRef, ReactNode } from 'react'

import { errorMessage } from '../lib/errors'
import { clearSettingsPreferences } from '../lib/settings-preferences'
import {
  settingsMatchRanges,
  type SettingsSearchResult,
  type SettingsSectionId,
} from '../lib/settings-search'
import { RotateCcw, X } from './icons'
import { Button } from './ui'

const SettingsSearchContext = createContext<SettingsSearchResult | null>(null)

export const SettingsSearchProvider = SettingsSearchContext.Provider

export function useSettingsSearch() {
  return useContext(SettingsSearchContext)
}

/** Marks the parts of a title that match the active Settings search. */
export function SettingsHighlight({ text }: { text: string }) {
  const search = useSettingsSearch()
  const ranges = search ? settingsMatchRanges(text, search.query) : []
  if (!ranges.length) return text
  const parts: ReactNode[] = []
  let cursor = 0
  for (const [start, end] of ranges) {
    if (start > cursor) parts.push(text.slice(cursor, start))
    parts.push(<mark key={start}>{text.slice(start, end)}</mark>)
    cursor = end
  }
  if (cursor < text.length) parts.push(text.slice(cursor))
  return parts
}

export function SettingsSection({
  id,
  title,
  action,
  children,
}: {
  id: SettingsSectionId
  title: string
  action?: ReactNode
  children: ReactNode
}) {
  const search = useSettingsSearch()
  return (
    <section
      id={id}
      className="settings-section"
      aria-labelledby={`${id}-heading`}
      tabIndex={-1}
      data-settings-section
      hidden={Boolean(search && !search.sectionIds.has(id))}
    >
      <header className="settings-section-header">
        <h2 id={`${id}-heading`}>{title}</h2>
        {search ? null : action}
      </header>
      {children}
    </section>
  )
}

/** A run of related rows inside a section; it disappears when search hides all of them. */
export function SettingsCard({ className, ...props }: ComponentPropsWithoutRef<'div'>) {
  return <div className={`settings-group${className ? ` ${className}` : ''}`} {...props} />
}

function useItemHidden(id: string) {
  const search = useSettingsSearch()
  return Boolean(search && !search.entryIds.has(id))
}

/** One label-and-control row. Title and consequence on the left, control on the right. */
export function SettingsRow({
  id,
  title,
  description,
  children,
  className,
  ...props
}: Omit<ComponentPropsWithoutRef<'div'>, 'title' | 'id'> & {
  id: string
  title: string
  description?: ReactNode
  children: ReactNode
}) {
  return (
    <div
      id={id}
      className={`settings-item settings-control-row${className ? ` ${className}` : ''}`}
      data-settings-item
      data-keyboard-row
      tabIndex={-1}
      hidden={useItemHidden(id)}
      {...props}
    >
      <span className="settings-control-copy">
        <strong id={`${id}-title`}>
          <SettingsHighlight text={title} />
        </strong>
        {description ? <small id={`${id}-description`}>{description}</small> : null}
      </span>
      <span className="settings-control-action">{children}</span>
    </div>
  )
}

/** A titled setting whose content (a list, table, or form) needs the full width. */
export function SettingsBlock({
  id,
  title,
  description,
  action,
  children,
  className,
}: {
  id: string
  title: string
  description?: ReactNode
  action?: ReactNode
  children?: ReactNode
  className?: string
}) {
  return (
    <div
      id={id}
      className={`settings-item settings-block${className ? ` ${className}` : ''}`}
      data-settings-item
      data-keyboard-row
      tabIndex={-1}
      hidden={useItemHidden(id)}
    >
      <div className="settings-block-header">
        <span className="settings-control-copy">
          <strong id={`${id}-title`}>
            <SettingsHighlight text={title} />
          </strong>
          {description ? <small>{description}</small> : null}
        </span>
        {action ? <span className="settings-control-action">{action}</span> : null}
      </div>
      {children}
    </div>
  )
}

/** Native checkbox semantics presented as a macOS-style switch. */
export function SettingsSwitch({
  checked,
  onCheckedChange,
  labelledBy,
  describedBy,
  disabled,
}: {
  checked: boolean
  onCheckedChange: (checked: boolean) => void
  labelledBy: string
  describedBy?: string
  disabled?: boolean
}) {
  return (
    <input
      type="checkbox"
      role="switch"
      className="settings-switch"
      checked={checked}
      disabled={disabled}
      aria-labelledby={labelledBy}
      aria-describedby={describedBy}
      onChange={(event) => onCheckedChange(event.target.checked)}
    />
  )
}

/** Section-scoped reset with an in-app confirmation (window.confirm cancels in WebKit). */
export function SettingsResetButton({
  section,
  keys,
  onReset,
  detail,
}: {
  section: string
  detail?: string
} & (
  | { keys: readonly string[]; onReset?: never }
  | { keys?: never; onReset: () => Promise<unknown> }
)) {
  const [confirming, setConfirming] = useState(false)
  const [pending, setPending] = useState(false)
  const [error, setError] = useState('')
  const trigger = useRef<HTMLButtonElement>(null)
  const cancel = useRef<HTMLButtonElement>(null)
  const returnFocus = useRef(false)
  useEffect(() => {
    if (confirming) cancel.current?.focus({ preventScroll: true })
    else if (returnFocus.current) trigger.current?.focus({ preventScroll: true })
    returnFocus.current = false
  }, [confirming])
  if (!confirming)
    return (
      <Button
        ref={trigger}
        icon={RotateCcw}
        size="compact"
        variant="ghost"
        className="settings-reset"
        onClick={() => setConfirming(true)}
      >
        Reset
      </Button>
    )
  return (
    <span className="settings-reset-confirmation" role="group" aria-label={`Reset ${section}`}>
      <small role={error ? 'alert' : undefined}>
        {error || (detail ?? `Restore ${section} defaults?`)}
      </small>
      <Button
        ref={cancel}
        icon={X}
        size="compact"
        disabled={pending}
        onClick={() => {
          returnFocus.current = true
          setError('')
          setConfirming(false)
        }}
      >
        Cancel
      </Button>
      <Button
        icon={RotateCcw}
        size="compact"
        variant="destructive"
        aria-busy={pending || undefined}
        disabled={pending}
        onClick={() => {
          if (keys) return clearSettingsPreferences(keys)
          setPending(true)
          setError('')
          void onReset()
            .then(() => {
              returnFocus.current = true
              setConfirming(false)
            })
            .catch((reason) => setError(errorMessage(reason)))
            .finally(() => setPending(false))
        }}
      >
        {pending ? 'Resetting…' : 'Reset'}
      </Button>
    </span>
  )
}

/**
 * An in-app confirmation under the control that opened it (window.confirm cancels in WebKit):
 * Cancel takes focus as it opens, Escape cancels, and focus goes back to `returnFocus` when it
 * closes, since the control that opened it has often been replaced in the meantime.
 */
export function InlineConfirm({
  label,
  labelledBy,
  id,
  className,
  pending = false,
  onCancel,
  returnFocus,
  actions,
  children,
}: {
  label?: string
  labelledBy?: string
  id?: string
  className?: string
  /** An action is running: Cancel and Escape wait for it. */
  pending?: boolean
  onCancel: () => void
  returnFocus: () => HTMLElement | null | undefined
  /** The buttons after Cancel. */
  actions: ReactNode
  children: ReactNode
}) {
  const group = useRef<HTMLDivElement>(null)
  const cancel = useRef<HTMLButtonElement>(null)
  const restore = useRef(returnFocus)
  useLayoutEffect(() => {
    restore.current = returnFocus
  })
  useLayoutEffect(() => {
    cancel.current?.focus({ preventScroll: true })
    const node = group.current
    return () => {
      if (!node?.contains(document.activeElement)) return
      // After the commit, once removal has dropped focus to the body, and only then.
      queueMicrotask(() => {
        if (document.activeElement && document.activeElement !== document.body) return
        restore.current()?.focus({ preventScroll: true })
      })
    }
  }, [])
  return (
    <div
      ref={group}
      id={id}
      className={`connection-confirmation${className ? ` ${className}` : ''}`}
      role="group"
      aria-label={label}
      aria-labelledby={labelledBy}
      onKeyDown={(event) => {
        if (event.key !== 'Escape') return
        event.preventDefault()
        event.stopPropagation()
        if (!pending) onCancel()
      }}
    >
      {children}
      <div>
        <Button ref={cancel} icon={X} size="compact" disabled={pending} onClick={onCancel}>
          Cancel
        </Button>
        {actions}
      </div>
    </div>
  )
}

export function SettingsNotice({ children }: { children: ReactNode }) {
  return <p className="settings-copy settings-notice">{children}</p>
}
