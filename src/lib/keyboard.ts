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
