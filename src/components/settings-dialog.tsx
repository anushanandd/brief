import { memo, Suspense, use, useEffect, useId } from 'react'
import type { ComponentProps, ReactNode } from 'react'

import { useSettingsDialog } from '../lib/settings-dialog'

type SettingsModule = typeof import('../pages/settings')
type TrackedImport = Promise<SettingsModule> & {
  status?: 'pending' | 'fulfilled' | 'rejected'
  value?: SettingsModule
  reason?: unknown
}
let settingsModule: TrackedImport | undefined

/**
 * One shared import whose status React's `use` can read. Once it has loaded, Settings renders
 * in the same commit that opens the dialog. `React.lazy` would suspend on first render even
 * when loaded, and React 19 then holds the reveal for up to 300 ms, showing a blank panel.
 */
export function loadSettingsPanel(): Promise<SettingsModule> {
  if (settingsModule) return settingsModule
  const tracked: TrackedImport = import('../pages/settings')
  tracked.status = 'pending'
  tracked.then(
    (module) => {
      tracked.status = 'fulfilled'
      tracked.value = module
    },
    (error: unknown) => {
      tracked.status = 'rejected'
      tracked.reason = error
      // A failed chunk load is retried the next time Settings opens.
      if (settingsModule === tracked) settingsModule = undefined
    },
  )
  settingsModule = tracked
  return tracked
}

function SettingsPanel(props: ComponentProps<SettingsModule['SettingsPanel']>) {
  const { SettingsPanel: Panel } = use(loadSettingsPanel())
  return <Panel {...props} />
}

/** Escape closes Settings from anywhere in it, except while a menu inside it is open. */
export function escapeClosesSettings(target: EventTarget | null) {
  return !(
    target instanceof Element &&
    target.closest('[role="listbox"], [role="menu"], [aria-expanded="true"]')
  )
}

/**
 * Settings as a page in the main pane, addressed by the hash (`#settings/<section-or-row>`) so
 * it opens from anywhere and closes back to the page beneath. Escape closes it unless a menu
 * or another dialog has the key. Memoized because it has no props: the shell's own state
 * changes never reach its sections.
 */
export const SettingsView = memo(function SettingsView() {
  const { target, show, close } = useSettingsDialog()
  const titleId = useId()

  useEffect(() => {
    const closeOnEscape = (event: KeyboardEvent) => {
      if (event.key !== 'Escape' || event.isComposing || event.defaultPrevented) return
      if (!escapeClosesSettings(event.target)) return
      if (event.target instanceof Element && event.target.closest('dialog, [role="dialog"]')) return
      event.preventDefault()
      close()
    }
    window.addEventListener('keydown', closeOnEscape)
    return () => window.removeEventListener('keydown', closeOnEscape)
  }, [close])

  return (
    <div
      className="page settings-page"
      role="region"
      aria-labelledby={titleId}
      aria-keyshortcuts="Escape"
    >
      <Suspense fallback={<div className="settings-panel settings-panel-loading" />}>
        <SettingsPanel target={target} titleId={titleId} onShow={show} onClose={close} />
      </Suspense>
    </div>
  )
})

/** A link that opens Settings at a section or row without leaving the current page. */
export function SettingsLink({
  target,
  className,
  children,
}: {
  target?: string
  className?: string
  children: ReactNode
}) {
  const { show } = useSettingsDialog()
  return (
    <a
      href={`#settings${target ? `/${target}` : ''}`}
      className={className}
      data-command="settings.open"
      onClick={(event) => {
        event.preventDefault()
        show(target)
      }}
    >
      {children}
    </a>
  )
}
