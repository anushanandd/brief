import { Link } from '@tanstack/react-router'

import { formatCurrency, formatPercent, valueTone } from '../lib/format'
import type { ChangeWindow, FinanceSnapshot } from '../lib/schema'
import { Metric } from './ui'

const periodLabels: Record<ChangeWindow, string> = {
  day: 'Today',
  week: 'Past week',
  month: 'Past month',
  quarter: 'Past 3 months',
  year: 'Past year',
  all: 'All time',
}

/** A figure's change from the period before, toned by whether its direction is favorable. */
function PeriodChange({ value, favorable }: { value?: number; favorable: 'up' | 'down' }) {
  if (value == null) return null
  return (
    <small
      className={valueTone(favorable === 'up' ? value : -value)}
      aria-label={`${formatPercent(value)} versus the period before`}
    >
      {formatPercent(value)}
    </small>
  )
}

/** Income, spending, and net over the period the chart shows, each with its change from the
    period before, all from native insights for that window. */
export function PeriodTotals({ data, window }: { data: FinanceSnapshot; window: ChangeWindow }) {
  const recap = data.insights?.recap.find((item) => item.window === window)
  const range = recap ? { from: recap.start, to: recap.end } : {}
  return (
    // The chart's range names the period, so the section carries it only as its accessible name.
    <section className="period-totals" aria-label={periodLabels[window]}>
      {recap?.partial ? (
        <small className="period-totals-partial" title="Saved activity starts later.">
          Partial period
        </small>
      ) : null}
      <div className="account-summary-metrics">
        <Metric
          label="Income"
          value={
            recap ? (
              <span className="period-totals-value">
                <Link
                  to="/analytics"
                  search={{ chart: 'income', ...range }}
                  className="metric-link sensitive-value"
                >
                  {formatCurrency(recap.income)}
                </Link>
                <PeriodChange value={recap.incomeChangePct} favorable="up" />
              </span>
            ) : (
              '—'
            )
          }
        />
        <Metric
          label="Spent"
          value={
            recap ? (
              <span className="period-totals-value">
                <Link to="/spending" className="metric-link sensitive-value">
                  {formatCurrency(recap.spending)}
                </Link>
                <PeriodChange value={recap.spendingChangePct} favorable="down" />
              </span>
            ) : (
              '—'
            )
          }
        />
        <Metric
          label="Net"
          value={
            recap ? (
              <span className="period-totals-value">
                <Link
                  to="/analytics"
                  search={{ chart: 'cash-flow', ...range }}
                  className="metric-link sensitive-value"
                >
                  {formatCurrency(recap.net)}
                </Link>
                <PeriodChange value={recap.netChangePct} favorable="up" />
              </span>
            ) : (
              '—'
            )
          }
        />
      </div>
    </section>
  )
}
