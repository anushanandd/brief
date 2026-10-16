import { Fragment, useMemo } from 'react'

import {
  actionBindings,
  actionScopePriority,
  bindingStartsWith,
  type AppAction,
} from '../lib/action-system'
import { X } from './icons'
import { Keys } from './keys'
import { Button } from './ui'

const sectionTitle = {
  selection: 'Selection',
  region: 'This list',
  page: 'This page',
  app: 'Anywhere',
} as const

type GuideEntry = {
  action: AppAction
  keys: string[]
}

export function KeyGuide({
  actions,
  prefix,
  count,
  onRun,
  onClose,
  onShowAll,
}: {
  actions: readonly AppAction[]
  prefix: readonly string[]
  count: string
  onRun: (action: AppAction) => void
  onClose: () => void
  onShowAll?: () => void
}) {
  // One row per action; an action with several bindings lists them as alternatives.
  const entries = useMemo(() => {
    const byAction = new Map<string, GuideEntry>()
    for (const { action, binding } of actionBindings(actions)) {
      // Before a sequence starts, the guide covers only this page; ⌘/ lists everything.
      if (!prefix.length && action.scope === 'app') continue
      if (!bindingStartsWith(binding, prefix)) continue
      const remaining = binding.sequence.slice(prefix.length)
      if (!remaining.length && prefix.length) continue
      const label = prefix.length ? remaining.join(' ') : binding.label
      const entry = byAction.get(action.id) ?? { action, keys: [] }
      if (!entry.keys.includes(label)) entry.keys.push(label)
      byAction.set(action.id, entry)
    }
    return [...byAction.values()].toSorted(
      (left, right) =>
        actionScopePriority[right.action.scope] - actionScopePriority[left.action.scope] ||
        left.action.title.localeCompare(right.action.title),
    )
  }, [actions, prefix])
  const typed = [...(count ? [count] : []), ...prefix].join(' ')

  return (
    <aside className="key-guide" role="dialog" aria-modal="false" aria-labelledby="key-guide-title">
      <header className="key-guide-header">
        <h2 id="key-guide-title" className={typed ? 'key-guide-prefix' : undefined}>
          {typed ? (
            <>
              <Keys label={typed} />
              <span>then…</span>
            </>
          ) : (
            'Shortcuts'
          )}
        </h2>
        <Button
          icon={X}
          size="icon-compact"
          variant="ghost"
          aria-label="Close keyboard guide"
          onClick={onClose}
        />
      </header>
      <div className="key-guide-sections">
        {!entries.length ? <p className="key-guide-empty">No shortcuts on this page.</p> : null}
        {(['selection', 'region', 'page', 'app'] as const).map((scope) => {
          const scoped = entries.filter(({ action }) => action.scope === scope)
          if (!scoped.length) return null
          return (
            <section className="key-guide-section" key={scope}>
              <h3>{sectionTitle[scope]}</h3>
              <ul>
                {scoped.map(({ action, keys }) => (
                  <li key={action.id}>
                    <button
                      type="button"
                      disabled={action.disabled}
                      title={action.disabledReason}
                      onClick={() => onRun(action)}
                    >
                      <span className="key-guide-title">{action.title}</span>
                      <span className="key-guide-keys">
                        {keys.map((label, index) => (
                          <Fragment key={label}>
                            {index ? <span className="shortcuts-or">or</span> : null}
                            <Keys label={label} />
                          </Fragment>
                        ))}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </section>
          )
        })}
      </div>
      <footer>
        <span>
          <Keys label="Esc" /> Close
        </span>
        {prefix.length ? (
          <span>
            <Keys label="⌫" /> Back
          </span>
        ) : null}
        {onShowAll ? (
          <button type="button" className="key-guide-all" onClick={onShowAll}>
            All shortcuts <Keys label="⌘/" />
          </button>
        ) : null}
      </footer>
    </aside>
  )
}
