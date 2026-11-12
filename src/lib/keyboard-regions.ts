import { List } from '../components/icons'
import type { AppAction } from './action-system'

/** The list (`data-keyboard-region`) holding focus, if any. */
export function currentKeyboardRegion(): HTMLElement | undefined {
  const focused = document.activeElement instanceof HTMLElement ? document.activeElement : null
  return focused?.closest<HTMLElement>('[data-keyboard-region]') ?? undefined
}

export function visibleKeyboardRows(region: HTMLElement) {
  return [...region.querySelectorAll<HTMLElement>('[data-keyboard-row]')].filter(
    (row) => row.getClientRects().length > 0,
  )
}

/** The row one step from `current` in `direction`, wrapping at either end. */
export function adjacentRowIndex(current: number, length: number, direction: -1 | 1) {
  if (length <= 0) return -1
  return (current + direction + length) % length
}

const rowControl =
  'button:not(:disabled), a[href], input:not(:disabled), select:not(:disabled), textarea:not(:disabled), [tabindex="0"]'

/**
 * Opens the focused row through its `data-keyboard-open` target. A row without one, such as a
 * setting, is entered instead: its first control takes focus, ready for Space or Return.
 */
export function openFocusedKeyboardRow() {
  const row = document.activeElement?.closest<HTMLElement>('[data-keyboard-row]')
  const target = row?.matches('[data-keyboard-open]')
    ? row
    : row?.querySelector<HTMLElement>('[data-keyboard-open]')
  if (target) target.click()
  else if (document.activeElement === row) row?.querySelector<HTMLElement>(rowControl)?.focus()
}

/** Motions over the focused list's rows: next, previous, to either end, and open. */
export function contextualListActions(region: HTMLElement | undefined): AppAction[] {
  if (!region || !visibleKeyboardRows(region).length) return []
  const label = region.getAttribute('aria-label') || 'focused list'
  // Rows are read when an action runs, since a list can grow or shrink while focus stays in it.
  const move = (direction: -1 | 1) => {
    const rows = visibleKeyboardRows(region)
    const current = document.activeElement?.closest<HTMLElement>('[data-keyboard-row]')
    const index = current ? rows.indexOf(current) : direction > 0 ? -1 : 0
    const next = rows[adjacentRowIndex(index, rows.length, direction)]
    if (!next) return
    next.focus()
    next.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }
  const edge = (position: 'first' | 'last') => {
    const rows = visibleKeyboardRows(region)
    const row = position === 'first' ? rows[0] : rows.at(-1)
    row?.focus()
    row?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  }
  return [
    {
      id: `motion.next:${label}`,
      title: `Next item in ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [
        { sequence: ['j'], label: 'J' },
        { sequence: ['ArrowDown'], label: '↓' },
      ],
      run: () => move(1),
    },
    {
      id: `motion.previous:${label}`,
      title: `Previous item in ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [
        { sequence: ['k'], label: 'K' },
        { sequence: ['ArrowUp'], label: '↑' },
      ],
      run: () => move(-1),
    },
    {
      id: `motion.first:${label}`,
      title: `First item in ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [{ sequence: ['g', 'g'], label: 'g g' }],
      run: () => edge('first'),
    },
    {
      id: `motion.last:${label}`,
      title: `Last item in ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [{ sequence: ['G'], label: 'G' }],
      run: () => edge('last'),
    },
    {
      id: `motion.open:${label}`,
      title: `Open item in ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [
        { sequence: ['Enter'], label: '↵' },
        { sequence: ['o'], label: 'O' },
      ],
      run: openFocusedKeyboardRow,
    },
  ]
}
