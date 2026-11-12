const inputSelector =
  'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="combobox"], [role="listbox"], [role="option"], [role="menu"], [role="menuitem"], [role="radio"], [role="slider"], [aria-haspopup="listbox"]'

export function shortcutInput(target: EventTarget | null) {
  return target instanceof Element && Boolean(target.closest(inputSelector))
}

export function shortcutOverlayOpen() {
  return Array.from(
    document.querySelectorAll(
      'dialog[open], [role="dialog"][aria-modal="true"], [data-popup-open]',
    ),
  ).some((element) => !element.hasAttribute('data-base-ui-tooltip-trigger'))
}

/**
 * The open modal dialog that owns the keyboard: the one holding focus, else the last opened.
 * A native modal makes everything outside it inert, so shortcuts that reach into the page,
 * such as list motions and focus hints, act inside it instead.
 */
export function openModal(): HTMLElement | undefined {
  const modals = [
    ...document.querySelectorAll<HTMLElement>('dialog[open], [role="dialog"][aria-modal="true"]'),
  ]
  const focused = document.activeElement
  return modals.find((modal) => focused && modal.contains(focused)) ?? modals.at(-1)
}

export function pageShortcutBlocked(
  event?: Pick<Event, 'defaultPrevented' | 'target'> & { isComposing?: boolean },
) {
  return (
    event?.defaultPrevented ||
    event?.isComposing ||
    shortcutOverlayOpen() ||
    shortcutInput(event ? event.target : document.activeElement)
  )
}
