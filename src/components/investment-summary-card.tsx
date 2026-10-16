import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ZodError } from 'zod'

import { getInvestmentSummary, isTauri } from '../lib/api'
import { type ChartPeriod } from '../lib/graph-preferences'
import { summaryStart } from '../lib/investment-summary'
import type { FinanceSnapshot } from '../lib/schema'
import { BriefLine } from './brief-line'
import { AsOf, Headlines } from './investment-summary-parts'
import { Card, EmptyState, SectionHeading } from './ui'

export const summaryKey = (
  start: string | undefined,
  ticker: string | undefined,
  revision?: number,
) => ['investment-summary', start, ticker ?? '', revision]

/** A summary with its figures, for the period the page shows. */
export function useInvestmentSummary(data: FinanceSnapshot, range: ChartPeriod, ticker?: string) {
  const native = typeof window !== 'undefined' && isTauri()
  const start = summaryStart(data, range)
  const summary = useQuery({
    queryKey: summaryKey(start, ticker, data.revision),
    queryFn: () => getInvestmentSummary(start!, ticker),
    // The browser preview has the portfolio's summary from the synthetic seed.
    enabled: (native || !ticker) && Boolean(start),
    staleTime: 30 * 60_000,
    retry: false,
  })
  return { start, summary }
}

/** Builds the portfolio's summary for the page's period in the background, on launch. */
export async function warmInvestmentSummary(
  client: ReturnType<typeof useQueryClient>,
  data: FinanceSnapshot,
  range: ChartPeriod,
) {
  const start = summaryStart(data, range)
  if (!start) return
  await client.fetchQuery({
    queryKey: summaryKey(start, undefined, data.revision),
    queryFn: () => getInvestmentSummary(start),
    staleTime: 30 * 60_000,
  })
}

/**
 * Today in plain words: how the investments moved against the market and who led, then the
 * news that explains it. Each holding's move, the period, and events have their own cards.
 */
export function InvestmentSummaryCard({
  data,
  range,
  ticker,
}: {
  data: FinanceSnapshot
  range: ChartPeriod
  ticker?: string
}) {
  const native = typeof window !== 'undefined' && isTauri()
  const { start, summary } = useInvestmentSummary(data, range, ticker)
  const result = summary.data
  const lines = result?.brief?.lines ?? []

  return (
    <Card className="portfolio-card investment-summary-card">
      <SectionHeading title="Summary" action={result ? <AsOf summary={result} /> : undefined} />
      {!native && ticker ? (
        <EmptyState>Needs the desktop app.</EmptyState>
      ) : !start ? (
        <EmptyState>No return history for this period.</EmptyState>
      ) : summary.isLoading ? (
        <div className="investment-brief-pending" role="status" aria-label="Gathering the day">
          <span className="skeleton" />
          <span className="skeleton" />
        </div>
      ) : summary.isError ? (
        <EmptyState>
          {summary.error instanceof Error && !(summary.error instanceof ZodError)
            ? summary.error.message
            : 'Couldn’t summarize the day.'}
        </EmptyState>
      ) : result && lines.length ? (
        <>
          <p className="investment-brief">
            {lines.map((spans, index) => (
              <span key={index}>
                {index ? ' ' : null}
                <BriefLine spans={spans} />
              </span>
            ))}
          </p>
          <Headlines summary={result} />
        </>
      ) : (
        <EmptyState>Today’s prices aren’t available yet.</EmptyState>
      )}
    </Card>
  )
}
