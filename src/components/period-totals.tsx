import { Link } from '@tanstack/react-router'

import { formatCurrency, formatPercent, valueTone } from '../lib/format'
import type { ChangeWindow, FinanceSnapshot } from '../lib/schema'
import { Metric } from './ui'

const periodLabels: Record<ChangeWindow, string> = {
  week: 'Past week',
  month: 'Past month',
  quarter: 'Past 3 months',
  year: 'Past year',
  all: 'All time',
}

/** Spent and income over the period the chart shows, from native insights for that window. */
export function PeriodTotals({ data, window }: { data: FinanceSnapshot; window: ChangeWindow }) {
  const recap = data.insights?.recap.find((item) => item.window === window)
  const change =
    recap?.previousSpending == null || !recap.previousSpending
      ? null
      : ((recap.spending - recap.previousSpending) / recap.previousSpending) * 100
  return (
    <section className="period-totals" aria-labelledby="period-totals-title">
      <h3 id="period-totals-title">
        {periodLabels[window]}
        {change == null ? null : <small> · vs the period before</small>}
        {recap?.partial ? <small title="Saved activity starts later."> · partial</small> : null}
      </h3>
      <div className="account-summary-metrics">
        <Metric
          label="Spent"
          value={
            recap ? (
              <span className="period-totals-value">
                <Link to="/spending" className="metric-link sensitive-value">
                  {formatCurrency(recap.spending)}
                </Link>
                {change == null ? null : (
                  // Less spending than the period before is the favorable direction.
                  <small
                    className={valueTone(-change)}
                    aria-label={`${formatPercent(change)} versus the period before`}
                  >
                    {formatPercent(change)}
                  </small>
                )}
              </span>
            ) : (
              '—'
            )
          }
        />
        <Metric
          label="Income"
          value={
            recap ? (
              <Link
                to="/analytics"
                search={{ chart: 'income', from: recap.start, to: recap.end }}
                className="metric-link sensitive-value"
              >
                {formatCurrency(recap.income)}
              </Link>
            ) : (
              '—'
            )
          }
        />
      </div>
    </section>
  )
}
