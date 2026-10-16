import { memo, Suspense, use, useEffect, useId, useRef, useState } from 'react'
import type { ComponentProps, ReactNode } from 'react'

import { useSettingsDialog } from '../lib/settings-dialog'
import { PopupContainerContext } from './ui/popup-container'
import { usePresence } from './ui/presence'

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
 * A click lands on the dialog element itself both on the backdrop and when a menu option
 * unmounts under the pointer, which retargets the click to the common ancestor. Only a press
 * that also started on the dialog element is a backdrop click.
 */
export function backdropClick(
  dialog: EventTarget,
  pressTarget: EventTarget | null,
  clickTarget: EventTarget | null,
) {
  return pressTarget === dialog && clickTarget === dialog
}

/**
 * Settings as a modal over the current page. The native dialog supplies the top layer,
 * inert background, Escape handling, and focus return to the control that opened it.
 * Memoized because it has no props: the shell's own state changes never reach its sections.
 */
export const SettingsDialog = memo(function SettingsDialog() {
  const { open, target, show, close } = useSettingsDialog()
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  const titleId = useId()
  const pressTarget = useRef<EventTarget | null>(null)
  // The panel stays while the dialog settles back down.
  const present = usePresence(open, dialog)

  useEffect(() => {
    if (!dialog || typeof dialog.showModal !== 'function') return
    if (open && !dialog.open) dialog.showModal()
    else if (!open && dialog.open) dialog.close()
  }, [dialog, open])

  return (
    <dialog
      ref={setDialog}
      className="settings-dialog"
      aria-labelledby={titleId}
      aria-keyshortcuts="Escape"
      onCancel={(event) => {
        event.preventDefault()
        close()
      }}
      onKeyDown={(event) => {
        // WebKit's search field would otherwise consume Escape to clear itself.
        if (event.key !== 'Escape' || event.nativeEvent.isComposing) return
        if (!escapeClosesSettings(event.target)) return
        event.preventDefault()
        close()
      }}
      onPointerDown={(event) => {
        pressTarget.current = event.target
      }}
      onClick={(event) => {
        const pressed = pressTarget.current
        pressTarget.current = null
        if (backdropClick(event.currentTarget, pressed, event.target)) close()
      }}
    >
      {present ? (
        <PopupContainerContext.Provider value={dialog}>
          <Suspense fallback={<div className="settings-panel settings-panel-loading" />}>
            <SettingsPanel target={target} titleId={titleId} onShow={show} onClose={close} />
          </Suspense>
        </PopupContainerContext.Provider>
      ) : null}
    </dialog>
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
