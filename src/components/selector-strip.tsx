import { Link, useNavigate, useRouter, type LinkOptions } from '@tanstack/react-router'
import { useCallback, useEffect, useRef, type ReactNode } from 'react'

import { useActiveSelectionScroll } from '../hooks/use-active-selection-scroll'
import { useGraphAccountShortcuts } from '../hooks/use-graph-window-shortcuts'
import {
  changeTone,
  formatCurrency,
  formatSignedCompactCurrency,
  formatSignedCurrency,
} from '../lib/format'

export type SelectorStripItem = {
  id: string
  label: string
  mark: ReactNode
  link: LinkOptions
  current: boolean
  /** The item's figure for the page's period; null reads as unavailable. */
  value: number | null
  /** The change over the page's period, in dollars; omitted when there is none to compare. */
  change?: number | null
  /** Spending and fees improve as they fall. */
  favorable?: 'increase' | 'decrease'
  /** The item can't report for this period. */
  unavailable?: boolean
  /** What the figure measures, when the label alone doesn't say ("sent", "a month"). */
  detail?: string
}

/**
 * The one selector at the head of Accounts, Money, and Investments: a strip of cards, each with
 * its figure and change over the page's period. ⌘↑ and ⌘↓ step through it.
 */
export function SelectorStrip({
  label,
  items,
  stepping = true,
}: {
  label: string
  items: SelectorStripItem[]
  /** Off while the view below takes ⌘↑ and ⌘↓ for its own items. */
  stepping?: boolean
}) {
  const ref = useActiveSelectionScroll()
  const navigate = useNavigate()
  const step = useCallback(
    (direction: -1 | 1) => {
      if (items.length < 2) return
      const index = Math.max(
        0,
        items.findIndex(({ current }) => current),
      )
      const target = items[(index + direction + items.length) % items.length]
      if (target) void navigate(target.link)
    },
    [items, navigate],
  )
  useGraphAccountShortcuts(step, stepping)
  // The cards either side are the likeliest next step, so their code and data load ahead and
  // ⌘↑ or ⌘↓ only has to render.
  const router = useRouter()
  const index = items.findIndex(({ current }) => current)
  const neighbours =
    index < 0 || items.length < 2
      ? []
      : [
          items[(index - 1 + items.length) % items.length],
          items[(index + 1) % items.length],
        ].filter((item) => item !== undefined)
  // A new period changes what the neighbours open on, so it loads them again.
  const neighbourKey = JSON.stringify(neighbours.map(({ id, link }) => [id, link.search]))
  const latestNeighbours = useRef(neighbours)
  latestNeighbours.current = neighbours
  useEffect(() => {
    const links = latestNeighbours.current.map(({ link }) => link)
    if (!links.length) return undefined
    const idle = globalThis.requestIdleCallback ?? ((run: () => void) => setTimeout(run, 50))
    const cancel = globalThis.cancelIdleCallback ?? clearTimeout
    const handle = idle(() => {
      for (const link of links) void router.preloadRoute(link).catch(() => undefined)
    })
    return () => cancel(handle)
  }, [neighbourKey, router])

  return (
    <nav
      ref={ref}
      className="account-switcher"
      aria-label={label}
      aria-keyshortcuts={stepping ? 'Meta+ArrowUp Meta+ArrowDown' : undefined}
      data-keyboard-region
    >
      <div className="account-switcher-grid">
        {items.map((item) => (
          <Link
            key={item.id}
            {...item.link}
            className="account-switcher-button"
            activeOptions={{ exact: true }}
            activeProps={{}}
            aria-current={item.current ? 'page' : undefined}
            data-keyboard-row
            data-keyboard-open
          >
            {item.mark}
            <span className="account-switcher-copy">
              <span title={item.label}>{item.label}</span>
              <strong className="account-switcher-value">
                <span className="sensitive-value" data-copy-amount>
                  {formatCurrency(item.unavailable ? null : item.value)}
                </span>
                {item.detail && !item.unavailable ? <small>{item.detail}</small> : null}
                {item.change == null || item.unavailable ? null : (
                  <span
                    className={`sensitive-value ${changeTone(item.change, item.favorable)}`}
                    aria-label={`${formatSignedCurrency(item.change)} over the period`}
                  >
                    {formatSignedCompactCurrency(item.change)}
                  </span>
                )}
              </strong>
            </span>
          </Link>
        ))}
      </div>
    </nav>
  )
}

/** A page's or report's icon on the tinted disc that account marks use. */
export function StripIconMark({ icon, color }: { icon: ReactNode; color: string }) {
  return (
    <span
      className="transaction-mark strip-icon-mark"
      style={{ ['--mark-tone' as string]: color }}
      aria-hidden="true"
    >
      {icon}
    </span>
  )
}
