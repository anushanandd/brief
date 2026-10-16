import { useEffect, useId, useState } from 'react'

import { actionBindings, type AppAction } from '../lib/action-system'
import { X } from './icons'
import { Keys } from './keys'
import { Button } from './ui'
import { usePresence } from './ui/presence'

export type ShortcutRow = { title: string; keys: readonly string[] }
export type ShortcutSection = { title: string; rows: readonly ShortcutRow[] }

// Motions exist only while a list has focus, so they are documented here rather than read from
// the registry; the palette's own keys live inside the palette and never reach the registry.
const listRows: ShortcutRow[] = [
  { title: 'Next or previous item', keys: ['J', 'K'] },
  { title: 'Move down or up a page', keys: ['⌃D', '⌃U'] },
  { title: 'First or last item', keys: ['g g', 'G'] },
  { title: 'Repeat a motion', keys: ['3 J'] },
  { title: 'Repeat last action', keys: ['.'] },
]

const paletteRows: ShortcutRow[] = [
  { title: 'Change type', keys: ['←', '→'] },
  { title: 'Filter Activity', keys: ['/'] },
  { title: 'Actions for the selected result', keys: ['⌥↵'] },
  { title: 'Secondary action', keys: ['⌘↵'] },
  { title: 'Add or remove a favorite', keys: ['⌘D'] },
  { title: 'Ask Brief', keys: ['?'] },
]

const sectionOrder = ['General', 'Navigation', 'This page', 'Appearance'] as const

function sectionFor(action: AppAction): (typeof sectionOrder)[number] {
  if (action.scope !== 'app') return 'This page'
  if (action.group === 'Navigate') return 'Navigation'
  if (action.group === 'Appearance') return 'Appearance'
  return 'General'
}

/** Every bound action, grouped like the app's menus, with at most two bindings each. */
export function shortcutSections(actions: readonly AppAction[]): ShortcutSection[] {
  const rows = new Map<string, { section: string; row: ShortcutRow }>()
  for (const { action, binding } of actionBindings(actions)) {
    if (action.id.startsWith('motion.') || action.id === 'action.repeat') continue
    const entry = rows.get(action.title) ?? {
      section: sectionFor(action),
      row: { title: action.title, keys: [] as string[] },
    }
    if (entry.row.keys.length < 2 && !entry.row.keys.includes(binding.label))
      entry.row = { ...entry.row, keys: [...entry.row.keys, binding.label] }
    rows.set(action.title, entry)
  }
  const grouped = sectionOrder.map((title) => ({
    title,
    rows: [...rows.values()].filter(({ section }) => section === title).map(({ row }) => row),
  }))
  return [
    ...grouped,
    { title: 'Lists', rows: listRows },
    { title: 'Command palette', rows: paletteRows },
  ].filter(({ rows: sectionRows }) => sectionRows.length)
}

/** The ⌘/ reference: every shortcut available now, in the native dialog's top layer. */
export function ShortcutsDialog({
  open,
  actions,
  onClose,
}: {
  open: boolean
  actions: readonly AppAction[]
  onClose: () => void
}) {
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  const titleId = useId()
  const present = usePresence(open, dialog)

  useEffect(() => {
    if (!dialog || typeof dialog.showModal !== 'function') return
    if (open && !dialog.open) dialog.showModal()
    else if (!open && dialog.open) dialog.close()
  }, [dialog, open])

  return (
    <dialog
      ref={setDialog}
      className="shortcuts-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      {present ? (
        <div className="shortcuts-panel">
          <header className="shortcuts-header">
            <h2 id={titleId}>Keyboard shortcuts</h2>
            <Button size="icon" variant="ghost" aria-label="Close" onClick={onClose}>
              <X size={16} aria-hidden="true" />
            </Button>
          </header>
          <div className="shortcuts-body">
            {shortcutSections(actions).map((section) => (
              <section className="shortcuts-section" key={section.title}>
                <h3>{section.title}</h3>
                <ul>
                  {section.rows.map((row) => (
                    <li key={row.title}>
                      <span>{row.title}</span>
                      <span className="shortcuts-keys">
                        {row.keys.map((label, index) => (
                          <span className="shortcuts-key" key={label}>
                            {index ? <span className="shortcuts-or">or</span> : null}
                            <Keys label={label} />
                          </span>
                        ))}
                      </span>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
        </div>
      ) : null}
    </dialog>
  )
}
