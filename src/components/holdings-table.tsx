import { Link, useNavigate, useRouter, type LinkOptions } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useState } from 'react'

import { useGraphAccountShortcuts } from '../hooks/use-graph-window-shortcuts'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import {
  countOf,
  formatCurrency,
  formatPercent,
  formatPublicCurrency,
  formatSecurityName,
  valueTone,
} from '../lib/format'
import { consolidatedPositions } from '../lib/holding-detail'
import type { FinanceSnapshot } from '../lib/schema'
import { nextSort } from '../lib/sort'
import { AccountMark } from './account-mark'
import { SecurityMark } from './security-mark'
import { SortHeadings } from './sort-headings'
import { Card, SectionHeading } from './ui'

/** The menu row that opens Retirement, which no ticker can collide with. */
export const RETIREMENT = 'retirement'

type Column = 'holding' | 'price' | 'today' | 'total' | 'cost' | 'value'
type Sort = { column: Column; direction: 'asc' | 'desc' }
type Holding = ReturnType<typeof consolidatedPositions>[number]

const columns: Array<{ id: Column; label: string }> = [
  { id: 'holding', label: 'Ticker' },
  { id: 'price', label: 'Price' },
  { id: 'today', label: 'Today %' },
  { id: 'total', label: 'Total %' },
  { id: 'cost', label: 'Cost basis' },
  { id: 'value', label: 'Value' },
]

const sortValue = (row: Holding, column: Exclude<Column, 'holding'>) =>
  ({
    price: row.price,
    today: row.dailyChangePct,
    total: row.totalChangePct,
    cost: row.costBasis,
    value: row.value,
  })[column]

/** Missing figures sort last in either direction; tickers sort alphabetically. */
function compare(left: Holding, right: Holding, { column, direction }: Sort) {
  const sign = direction === 'asc' ? 1 : -1
  if (column === 'holding') return sign * left.ticker.localeCompare(right.ticker)
  const a = sortValue(left, column)
  const b = sortValue(right, column)
  if (a == null || b == null) return a == null ? (b == null ? 0 : 1) : -1
  return sign * (a - b) || left.ticker.localeCompare(right.ticker)
}

const linkTo = (selected?: string): LinkOptions =>
  selected === undefined
    ? { to: '/holdings' }
    : selected === RETIREMENT
      ? { to: '/holdings/retirement' }
      : { to: '/holdings/$ticker', params: { ticker: selected } }

const Percent = ({ value }: { value: number | null }) => (
  <span className={valueTone(value)}>{formatPercent(value)}</span>
)

/**
 * Investments' menu, as Accounts has one: the portfolio, retirement, then every holding with
 * its price, its change today and since bought, its cost, and its worth. Each row changes the
 * view in place, the one shown is marked, and ⌘↑ and ⌘↓ step through them in the order shown.
 */
export function HoldingsTable({
  data,
  selected,
  value,
}: {
  data: FinanceSnapshot
  /** A ticker, `RETIREMENT`, or nothing for the portfolio. */
  selected?: string
  value: number | null
}) {
  const [sort, setSort] = useState<Sort>({ column: 'value', direction: 'desc' })
  const toggle = useCallback(
    (column: Column) =>
      setSort((current) => nextSort(current, column, column === 'holding' ? 'asc' : 'desc')),
    [],
  )
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        columns.map(({ id, label }) => ({
          id: `investments.sort.${id}`,
          title: `Sort holdings by ${label}`,
          search: `order holdings column ${label}`,
          group: 'View',
          scope: 'page' as const,
          run: () => toggle(id),
        })),
      [toggle],
    ),
  )

  const holdings = consolidatedPositions(data).toSorted((left, right) => compare(left, right, sort))
  // The menu's order, by what each row selects: the portfolio is none.
  const menu = [undefined, RETIREMENT, ...holdings.map(({ ticker }) => ticker)]
  const index = Math.max(0, menu.indexOf(selected))
  const at = (offset: number) => menu[(index + offset + menu.length) % menu.length]
  const navigate = useNavigate()
  useGraphAccountShortcuts((direction) => void navigate(linkTo(at(direction))))
  // The rows either side are the likeliest next step, so they load ahead and ⌘↑ or ⌘↓ only
  // has to render.
  const router = useRouter()
  const previous = at(-1)
  const next = at(1)
  useEffect(() => {
    const idle = globalThis.requestIdleCallback ?? ((run: () => void) => setTimeout(run, 50))
    const cancel = globalThis.cancelIdleCallback ?? clearTimeout
    const handle = idle(() => {
      for (const id of [previous, next]) void router.preloadRoute(linkTo(id)).catch(() => undefined)
    })
    return () => cancel(handle)
  }, [previous, next, router])

  return (
    <Card className="accounts-menu holdings-table-card">
      <SectionHeading title="Holdings" />
      <nav
        className="holdings-table-scroll"
        aria-label="Holdings"
        aria-keyshortcuts="Meta+ArrowUp Meta+ArrowDown"
        data-keyboard-region
      >
        <div className="holdings-table">
          <Link
            className="holdings-table-row holdings-table-scope"
            to="/holdings"
            data-keyboard-row
            data-keyboard-open
            aria-current={selected ? undefined : 'page'}
          >
            <span className="holdings-table-name">
              <AccountMark type="brokerage" />
              <span>
                <strong>Portfolio</strong>
                <small>{countOf(holdings.length, 'holding')}</small>
              </span>
            </span>
            <span className="sensitive-value" data-copy-amount>
              {formatCurrency(value)}
            </span>
          </Link>
          <Link
            className="holdings-table-row holdings-table-scope"
            to="/holdings/retirement"
            data-keyboard-row
            data-keyboard-open
            aria-current={selected === RETIREMENT ? 'page' : undefined}
          >
            <span className="holdings-table-name">
              <AccountMark type="retirement" />
              <span>
                <strong>Retirement</strong>
              </span>
            </span>
            {/* Every retirement account's value, as Rust summed it at the values shown. */}
            <span className="sensitive-value" data-copy-amount>
              {formatCurrency(data.insights?.rollup.retirement)}
            </span>
          </Link>
          <div className="holdings-table-head">
            <SortHeadings
              columns={columns}
              sort={sort}
              command="investments.sort."
              onSort={toggle}
            />
          </div>
          {holdings.map((row) => (
            <Link
              key={row.ticker}
              className="holdings-table-row"
              to="/holdings/$ticker"
              params={{ ticker: row.ticker }}
              data-keyboard-row
              data-keyboard-open
              aria-current={selected === row.ticker ? 'page' : undefined}
            >
              <span className="holdings-table-name">
                <SecurityMark ticker={row.ticker} name={row.name} />
                <span>
                  <strong>{row.ticker}</strong>
                  <small title={formatSecurityName(row.name)}>{formatSecurityName(row.name)}</small>
                </span>
              </span>
              <span className="public-market-value">{formatPublicCurrency(row.price)}</span>
              <Percent value={row.dailyChangePct} />
              <Percent value={row.totalChangePct} />
              <span className="sensitive-value">{formatCurrency(row.costBasis)}</span>
              <span className="sensitive-value" data-copy-amount>
                {formatCurrency(row.value)}
              </span>
            </Link>
          ))}
        </div>
      </nav>
    </Card>
  )
}
