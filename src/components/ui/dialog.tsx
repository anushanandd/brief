import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ComponentProps,
  type Ref,
} from 'react'

import { useRegisterActions, type AppAction } from '../../lib/action-system'
import { PopupContainerContext } from './popup-container'

/**
 * Whether Escape should close a dialog: not while a listbox, menu, or other expanded control
 * inside it holds the key to close itself first.
 */
export function escapeClosesDialog(target: EventTarget | null) {
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

const noActions: readonly AppAction[] = []

/**
 * Marks the element a `Dialog` focuses when it opens: `{...dialogAutofocus}` on a field, or on a
 * title with `tabIndex={-1}` so no control starts out ringed. Without it the first control wins.
 */
export const autofocusAttribute = 'data-dialog-autofocus'
export const dialogAutofocus = { 'data-dialog-autofocus': '' } as const

/**
 * Registers a dialog's own keys while it is open, scoped to it: they run only while it holds
 * the keyboard, and the key guide, ⌘/, and the palette list them like any other action.
 */
export function useDialogActions(
  dialog: Element | null,
  open: boolean,
  actions: readonly AppAction[],
) {
  useRegisterActions(
    useMemo(
      () =>
        open && dialog
          ? actions.map((action) => ({ ...action, scope: 'dialog' as const, dialog }))
          : noActions,
      [actions, dialog, open],
    ),
  )
}

type DialogProps = Omit<ComponentProps<'dialog'>, 'open' | 'ref' | 'onCancel' | 'onClick'> & {
  open: boolean
  /** Escape, a backdrop click, or the system's cancel. */
  onClose: () => void
  /**
   * Escape closes from keydown, which WebKit's search field would otherwise consume to clear
   * itself; an open menu or listbox inside keeps it.
   */
  escapeOnKeyDown?: boolean
  /** Popups opened inside portal into the dialog, so they show in its top layer. */
  popups?: boolean
  /** The dialog's own keys, registered while it is open; see `useDialogActions`. */
  actions?: readonly AppAction[]
  ref?: Ref<HTMLDialogElement>
}

/**
 * Brief's modal: a native <dialog>, which supplies the top layer, the inert page behind it,
 * and focus returning to whatever opened it. It opens and closes with `open`; the caller
 * decides when its content mounts (see `usePresence`), and every style comes from `className`.
 */
export function Dialog({
  open,
  onClose,
  escapeOnKeyDown = false,
  popups = false,
  actions = noActions,
  ref,
  onKeyDown,
  onPointerDown,
  children,
  ...props
}: DialogProps) {
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  const pressTarget = useRef<EventTarget | null>(null)
  const attach = useCallback(
    (node: HTMLDialogElement | null) => {
      setDialog(node)
      if (typeof ref === 'function') ref(node)
      else if (ref) ref.current = node
    },
    [ref],
  )
  useEffect(() => {
    if (!dialog || typeof dialog.showModal !== 'function') return undefined
    if (open && !dialog.open) {
      dialog.showModal()
      // `showModal` focuses the first control. React never writes the `autofocus` attribute,
      // and the element arrives through state, so this runs a render after the caller's own
      // open effects; the control that should start with focus is marked and focused here.
      dialog.querySelector<HTMLElement>(`[${autofocusAttribute}]`)?.focus()
    } else if (!open && dialog.open) dialog.close()
    return undefined
  }, [dialog, open])
  // A dialog unmounted while open leaves the top layer with it.
  useEffect(
    () => () => {
      if (dialog?.open) dialog.close()
    },
    [dialog],
  )
  useDialogActions(dialog, open, actions)
  return (
    <dialog
      ref={attach}
      {...props}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onKeyDown={(event) => {
        onKeyDown?.(event)
        if (
          !escapeOnKeyDown ||
          event.key !== 'Escape' ||
          event.nativeEvent.isComposing ||
          !escapeClosesDialog(event.target)
        )
          return
        event.preventDefault()
        onClose()
      }}
      onPointerDown={(event) => {
        pressTarget.current = event.target
        onPointerDown?.(event)
      }}
      onClick={(event) => {
        const pressed = pressTarget.current
        pressTarget.current = null
        if (backdropClick(event.currentTarget, pressed, event.target)) onClose()
      }}
    >
      {popups ? (
        <PopupContainerContext.Provider value={dialog}>{children}</PopupContainerContext.Provider>
      ) : (
        children
      )}
    </dialog>
  )
}
