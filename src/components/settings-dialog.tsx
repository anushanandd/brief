import { memo, Suspense, use, useId, useState } from 'react'
import type { ComponentProps, ReactNode } from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { useSettingsDialog } from '../lib/settings-dialog'
import { Dialog } from './ui/dialog'
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

/**
 * Settings as a modal over the current page, addressed by the hash (`#settings/<section-or-row>`)
 * so it opens from anywhere. The native dialog supplies the top layer, inert background, and
 * focus return to the control that opened it. Memoized so the shell's own state changes never
 * reach its sections, except screen privacy, which renders the panel again in place so every
 * amount is masked or shown without losing what's being typed.
 */
export const SettingsDialog = memo(function SettingsDialog() {
  useAmountsHidden()
  const { open, target, show, close } = useSettingsDialog()
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  const titleId = useId()
  // The panel stays while the dialog settles back down.
  const present = usePresence(open, dialog)

  return (
    <Dialog
      ref={setDialog}
      open={open}
      onClose={close}
      escapeOnKeyDown
      popups
      className="settings-dialog"
      aria-labelledby={titleId}
      aria-keyshortcuts="Escape"
    >
      {present ? (
        <Suspense fallback={<div className="settings-panel settings-panel-loading" />}>
          <SettingsPanel target={target} titleId={titleId} onShow={show} onClose={close} />
        </Suspense>
      ) : null}
    </Dialog>
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
