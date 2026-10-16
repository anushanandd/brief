import { createContext, useContext, useEffect, useRef, useState } from 'react'
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
  mark,
  children,
  className,
}: {
  id: string
  title: string
  mark?: ReactNode
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
      tabIndex={-1}
      hidden={useItemHidden(id)}
    >
      <div className="settings-block-header">
        <span className={mark ? 'settings-control-lead' : 'settings-control-copy'}>
          {mark}
          {mark ? (
            <span className="settings-control-copy">
              <strong id={`${id}-title`}>
                <SettingsHighlight text={title} />
              </strong>
              {description ? <small>{description}</small> : null}
            </span>
          ) : (
            <>
              <strong id={`${id}-title`}>
                <SettingsHighlight text={title} />
              </strong>
              {description ? <small>{description}</small> : null}
            </>
          )}
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

export function SettingsNotice({ children }: { children: ReactNode }) {
  return <p className="settings-copy settings-notice">{children}</p>
}
