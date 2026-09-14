import { X } from 'lucide-react'
import { useEffect, useRef } from 'react'

import { shortcutGroups } from '../lib/navigation'
import { Button } from './ui'

export function ShortcutHelp({ onClose }: { onClose: () => void }) {
  const dialog = useRef<HTMLDialogElement>(null)

  useEffect(() => {
    const element = dialog.current
    if (!element) return undefined
    element.showModal()
    return () => {
      if (element.open) element.close()
    }
  }, [])

  return (
    <dialog
      ref={dialog}
      className="command-dialog"
      aria-labelledby="shortcut-help-title"
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      <div className="command-popup shortcut-help-popup">
        <header className="shortcut-help-header">
          <h2 id="shortcut-help-title">Keyboard shortcuts</h2>
          <Button
            size="icon"
            variant="ghost"
            aria-label="Close keyboard shortcuts"
            onClick={onClose}
          >
            <X size={16} aria-hidden="true" />
          </Button>
        </header>
        <div className="shortcut-help-list">
          {shortcutGroups.map((group) => (
            <section className="shortcut-help-group" key={group.title}>
              <h3>{group.title}</h3>
              <dl>
                {group.shortcuts.map(([keys, label]) => (
                  <div key={keys}>
                    <dt>{label}</dt>
                    <dd>
                      <kbd>{keys}</kbd>
                    </dd>
                  </div>
                ))}
              </dl>
            </section>
          ))}
        </div>
      </div>
    </dialog>
  )
}
