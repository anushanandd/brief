import { Link } from '@tanstack/react-router'
import { useCallback, useMemo, useState } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import {
  formatCurrency,
  formatPercent,
  formatSecurityName,
  formatShare,
  formatSignedCurrency,
  valueTone,
} from '../lib/format'
import { type ChartPeriod } from '../lib/graph-preferences'
import type { FinanceSnapshot, InvestmentSummary, PortfolioToday } from '../lib/schema'
import { nextSort } from '../lib/sort'
import { Relevance } from './relevance'
import { SecurityMark } from './security-mark'
import { SortHeadings } from './sort-headings'
import { Card, EmptyState, SectionHeading } from './ui'

type Column = 'holding' | 'today' | 'period' | 'value' | 'weight' | 'gain' | 'news'
type Sort = { column: Column; direction: 'asc' | 'desc' }

const periodLabels: Record<ChartPeriod, string> = {
  week: 'Week',
  month: 'Month',
  quarter: '3 months',
  year: 'Year',
  all: 'All time',
}

type Row = {
  ticker: string
  name: string
  todayPct: number | null
  today: number | null
  periodPct: number | null
  period: number | null
  value: number | null
  weight: number | null
  gain: number | null
  gainPct: number | null
  stories: number
  relevance: number | null
}

const sortValue = (row: Row, column: Column) =>
  ({
    holding: null,
    today: row.today,
    period: row.period,
    value: row.value,
    weight: row.weight,
    gain: row.gain,
    news: row.relevance,
  })[column]

/** Missing figures sort last in either direction; names sort alphabetically. */
function compare(left: Row, right: Row, { column, direction }: Sort) {
  const sign = direction === 'asc' ? 1 : -1
  if (column === 'holding') return sign * left.ticker.localeCompare(right.ticker)
  const a = sortValue(left, column)
  const b = sortValue(right, column)
  if (a == null || b == null) return a == null ? (b == null ? 0 : 1) : -1
  return sign * (a - b) || left.ticker.localeCompare(right.ticker)
}

function Signed({ percent, amount }: { percent: number | null; amount: number | null }) {
  if (percent == null && amount == null) return <span className="muted">—</span>
  return (
    <span className="holdings-table-pair">
      <span className={valueTone(percent ?? amount)}>
        {percent == null ? '—' : formatPercent(percent)}
      </span>
      {amount == null ? null : (
        <small className="sensitive-value">{formatSignedCurrency(amount)}</small>
      )}
    </span>
  )
}

/**
 * Every holding with how it moved today and over the page's period, its worth and weight, its
 * gain, and how much relevant news it has. Rust computes each figure; the table only orders
 * them, by value until a column is chosen.
 */
export function HoldingsTable({
  data,
  range,
  today,
  summary,
}: {
  data: FinanceSnapshot
  range: ChartPeriod
  today?: PortfolioToday
  summary?: InvestmentSummary
}) {
  const [sort, setSort] = useState<Sort>({ column: 'value', direction: 'desc' })
  // Each holding's part in the period, from the summary's attribution.
  const contributions = summary?.report?.attribution?.contributions ?? []
  const periodLabel = periodLabels[range]
  const columns: Array<{ id: Column; label: string }> = useMemo(
    () => [
      { id: 'holding', label: 'Holding' },
      { id: 'today', label: 'Today' },
      { id: 'period', label: periodLabel },
      { id: 'value', label: 'Value' },
      { id: 'weight', label: 'Weight' },
      { id: 'gain', label: 'Gain' },
      { id: 'news', label: 'News' },
    ],
    [periodLabel],
  )
  const toggle = useCallback(
    (column: Column) =>
      setSort((current) => nextSort(current, column, column === 'holding' ? 'asc' : 'desc')),
    [],
  )
  const actions: AppAction[] = useMemo(
    () =>
      columns.map(({ id, label }) => ({
        id: `investments.sort.${id}`,
        title: `Sort holdings by ${label}`,
        search: `order holdings column ${label}`,
        group: 'View',
        scope: 'page' as const,
        run: () => toggle(id),
      })),
    [columns, toggle],
  )
  useRegisterActions(actions)

  const securities = data.insights?.securities ?? []
  const rows: Row[] = securities.map((security) => {
    const moved = today?.positions.find((position) => position.ticker === security.ticker)
    const driver = contributions.find((item) => item.key === security.ticker)
    const news = summary?.coverage.find((item) => item.ticker === security.ticker)
    return {
      ticker: security.ticker,
      name: formatSecurityName(security.name),
      todayPct: moved?.changePct ?? null,
      today: moved?.change ?? null,
      periodPct: driver?.returnPct ?? null,
      period: driver?.dollars ?? null,
      value: security.value ?? null,
      weight: security.percent ?? null,
      gain: security.gain ?? null,
      gainPct: security.gainPercent ?? null,
      stories: news?.stories ?? 0,
      relevance: news?.topRelevance ?? null,
    }
  })
  const sorted = rows.toSorted((left, right) => compare(left, right, sort))

  return (
    <Card className="portfolio-card holdings-table-card">
      <SectionHeading title="Holdings" />
      {!rows.length ? (
        <EmptyState>No holdings with a known value.</EmptyState>
      ) : (
        <div className="holdings-table-scroll" role="region" aria-label="Holdings">
          <div className="holdings-table">
            <div className="holdings-table-head">
              <SortHeadings
                columns={columns}
                sort={sort}
                command="investments.sort."
                onSort={toggle}
              />
            </div>
            <ol className="holdings-table-body" data-keyboard-region aria-label="Holdings">
              {sorted.map((row) => (
                <li key={row.ticker}>
                  <Link
                    className="holdings-table-row"
                    to="/holdings/$ticker"
                    params={{ ticker: row.ticker }}
                    data-keyboard-row
                    data-keyboard-open
                  >
                    <span className="holdings-table-name">
                      <SecurityMark ticker={row.ticker} name={row.name} small />
                      <span>
                        <strong>{row.ticker}</strong>
                        <small title={row.name}>{row.name}</small>
                      </span>
                    </span>
                    <Signed percent={row.todayPct} amount={row.today} />
                    <Signed percent={row.periodPct} amount={row.period} />
                    <span className="sensitive-value">
                      {row.value == null ? '—' : formatCurrency(row.value)}
                    </span>
                    <span className="holdings-table-weight">
                      {/* The track is the whole portfolio, so a 40% position draws 40% of it. */}
                      <span className="holdings-table-track" aria-hidden="true">
                        <span style={{ width: `${Math.min(100, row.weight ?? 0).toFixed(1)}%` }} />
                      </span>
                      <span>{formatShare(row.weight)}</span>
                    </span>
                    <Signed percent={row.gainPct} amount={row.gain} />
                    <span className="holdings-table-news">
                      {row.relevance == null ? (
                        <span className="muted">—</span>
                      ) : (
                        <>
                          <span>{row.stories}</span>
                          <Relevance value={row.relevance} />
                        </>
                      )}
                    </span>
                  </Link>
                </li>
              ))}
            </ol>
          </div>
        </div>
      )}
    </Card>
  )
}
