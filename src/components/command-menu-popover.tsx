import { useLayoutEffect, useState, type KeyboardEvent, type RefObject } from 'react'

import { groupKind, type MenuAction, type MenuRow } from '../lib/palette-rows'
import { Keys } from './keys'

const plainMeta = (event: KeyboardEvent, key: string) =>
  event.key.toLowerCase() === key && event.metaKey && !event.ctrlKey && !event.shiftKey

/**
 * The Actions menu beside the selected row, like claude.ai's: opened with ⌥↵ or ⌘K, walked
 * with arrows or digits, and closed by Escape or by typing on.
 */
export function useActionsMenu(
  items: readonly MenuRow[],
  frames: { dialog: RefObject<HTMLDialogElement | null>; list: RefObject<HTMLDivElement | null> },
  selectedValue: string,
  onClose: () => void,
) {
  const [open, setOpen] = useState(false)
  const [index, setIndex] = useState(0)
  const [position, setPosition] = useState<{ top: number; left: number }>()

  useLayoutEffect(() => {
    if (!open) return
    const row = frames.list.current?.querySelector('[cmdk-item][data-selected="true"]')
    const frame = frames.dialog.current?.getBoundingClientRect()
    if (!row || !frame) return
    // Beside the palette when there is room, otherwise over its right edge.
    const width = 248
    const fits = frame.right + 8 + width <= window.innerWidth - 12
    setPosition({
      top: row.getBoundingClientRect().top - frame.top,
      left: fits ? frame.width + 8 : frame.width - width - 8,
    })
  }, [frames.dialog, frames.list, open, selectedValue])

  const openMenu = () => {
    setIndex(0)
    setOpen(true)
  }
  const toggle = () => {
    setIndex(0)
    setOpen((current) => !current)
  }
  const close = () => setOpen(false)

  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const handled = () => {
      event.preventDefault()
      event.stopPropagation()
    }
    if (
      event.key === 'Escape' ||
      (event.key === 'Enter' && event.altKey) ||
      plainMeta(event, 'k')
    ) {
      handled()
      onClose()
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      handled()
      const offset = event.key === 'ArrowDown' ? 1 : -1
      setIndex((current) => (current + offset + items.length) % items.length)
    } else if (event.key === 'Enter') {
      handled()
      const row = items[index]
      if (row && !row.disabled) void row.run()
    } else if (/^[1-9]$/.test(event.key) && !event.metaKey && !event.ctrlKey) {
      handled()
      const row = items[Number(event.key) - 1]
      if (row && !row.disabled) void row.run()
    } else if (!event.metaKey) {
      // Typing closes the menu and keeps searching, as on claude.ai.
      setOpen(false)
    }
  }

  return { open, openMenu, toggle, close, index, setIndex, position, onKeyDown }
}

export function ActionsPopover({
  target,
  items,
  index,
  position,
  onHover,
}: {
  target: MenuAction
  items: readonly MenuRow[]
  index: number
  position: { top: number; left: number } | undefined
  onHover: (index: number) => void
}) {
  return (
    <div
      className="command-menu-popover"
      role="menu"
      aria-label={`Actions for ${target.label}`}
      style={position}
    >
      <div className="command-menu-header">
        <strong>{target.label}</strong>
        <small>
          {[groupKind[target.group] ?? 'Command', target.detail].filter(Boolean).join(' · ')}
        </small>
      </div>
      {items.map((row, rowIndex) => (
        <button
          type="button"
          role="menuitem"
          key={row.id}
          className={row.separated ? 'command-menu-separated' : undefined}
          data-active={rowIndex === index}
          disabled={row.disabled}
          tabIndex={-1}
          onMouseDown={(event) => event.preventDefault()}
          onMouseMove={() => onHover(rowIndex)}
          onClick={() => void row.run()}
        >
          <span>{row.label}</span>
          {rowIndex < 9 ? <Keys label={String(rowIndex + 1)} /> : null}
        </button>
      ))}
    </div>
  )
}
