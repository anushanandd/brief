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

/** The row `count` steps from `current` in `direction`, wrapping at either end. */
export function countedRowIndex(current: number, length: number, direction: -1 | 1, count = 1) {
  if (length <= 0) return -1
  return (current + direction * Math.max(1, count) + length * (count + 1)) % length
}

export function openFocusedKeyboardRow() {
  const row = document.activeElement?.closest<HTMLElement>('[data-keyboard-row]')
  const target = row?.matches('[data-keyboard-open]')
    ? row
    : row?.querySelector<HTMLElement>('[data-keyboard-open]')
  target?.click()
}

/** Motions over the focused list's rows: next, previous, by page, to either end, and open. */
export function contextualListActions(region: HTMLElement | undefined): AppAction[] {
  if (!region || !visibleKeyboardRows(region).length) return []
  const label = region.getAttribute('aria-label') || 'focused list'
  // Rows are read when an action runs, since a list can grow or shrink while focus stays in it.
  const move = (direction: -1 | 1, count = 1) => {
    const rows = visibleKeyboardRows(region)
    if (!rows.length) return
    const current = document.activeElement?.closest<HTMLElement>('[data-keyboard-row]')
    const index = current ? rows.indexOf(current) : direction > 0 ? -1 : 0
    const next = countedRowIndex(index, rows.length, direction, count)
    rows[next].focus()
    rows[next].scrollIntoView({ block: 'nearest', inline: 'nearest' })
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
        { sequence: ['j'], label: 'J', acceptsCount: true },
        { sequence: ['ArrowDown'], label: '↓', acceptsCount: true },
      ],
      repeatable: true,
      run: (count) => move(1, count),
    },
    {
      id: `motion.previous:${label}`,
      title: `Previous item in ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [
        { sequence: ['k'], label: 'K', acceptsCount: true },
        { sequence: ['ArrowUp'], label: '↑', acceptsCount: true },
      ],
      repeatable: true,
      run: (count) => move(-1, count),
    },
    {
      id: `motion.viewport-next:${label}`,
      title: `Move down through ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [{ sequence: ['Control+d'], label: '⌃D', acceptsCount: true }],
      repeatable: true,
      run: (count) => move(1, 8 * (count ?? 1)),
    },
    {
      id: `motion.viewport-previous:${label}`,
      title: `Move up through ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [{ sequence: ['Control+u'], label: '⌃U', acceptsCount: true }],
      repeatable: true,
      run: (count) => move(-1, 8 * (count ?? 1)),
    },
    {
      id: `motion.first:${label}`,
      title: `First item in ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [{ sequence: ['g', 'g'], label: 'g g' }],
      repeatable: true,
      run: () => edge('first'),
    },
    {
      id: `motion.last:${label}`,
      title: `Last item in ${label}`,
      group: 'Here',
      icon: List,
      scope: 'region',
      bindings: [{ sequence: ['G'], label: 'G' }],
      repeatable: true,
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
