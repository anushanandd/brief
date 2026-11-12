import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ZodError } from 'zod'

import { holdingsKey, type HoldingsIdentity } from '../hooks/use-finance'
import { getInvestmentSummary, isTauri, warmInvestmentSummaries } from '../lib/api'
import { type ChartPeriod } from '../lib/graph-preferences'
import { summaryStart } from '../lib/investment-summary'
import type { FinanceSnapshot } from '../lib/schema'
import { BriefLine } from './brief-line'
import { AsOf, Changed, Headlines, MarketWrap } from './investment-summary-parts'
import { Card, EmptyState, SectionHeading } from './ui'

type SummaryInputs = HoldingsIdentity & Pick<FinanceSnapshot, 'transactions'>

/**
 * What the summary reads from the snapshot, for its key: Rust builds it from the committed
 * holdings, trades, and dividends. Trades change only with a refresh or an import, which
 * advance `revision`; among edits a manual account or closed date changes which positions are
 * held; and a category edit can make a transaction a dividend or stop it being one, so the
 * dividends' IDs are part of the key. Prices are left out: the page's holdings carry live
 * prices that change with every tick, and Rust reads the committed snapshot, not these.
 */
export const summaryInputsKey = (data: SummaryInputs) => [
  holdingsKey(data)[0],
  data.holdings.map(({ accountId, ticker, shares }) => [accountId, ticker, shares]),
  data.transactions
    .filter((transaction) => transaction.classification?.mark === 'dividend')
    .map((transaction) => transaction.id),
]

export const summaryKey = (
  start: string | undefined,
  ticker: string | undefined,
  data: SummaryInputs,
) => ['investment-summary', start, ticker ?? '', ...summaryInputsKey(data)]

// Rust keeps each summary, so one answer stands for the session unless Rust says it changed.
const SUMMARY_STALE_MS = 30 * 60_000
// While Rust brings a saved summary up to date, it says so when done; this asks again in case
// that word was missed.
const REFRESHING_RECHECK_MS = 60_000

/**
 * A summary with its figures, for the period the page shows. Rust answers at once from the
 * summary it saved; one made from older inputs comes marked `refreshing` while Rust makes the
 * new one. `current` is the summary only when it isn't refreshing, for the cards that set its
 * figures beside today's; the summary card shows a refreshing one, marked.
 */
export function useInvestmentSummary(data: FinanceSnapshot, range: ChartPeriod, ticker?: string) {
  const native = typeof window !== 'undefined' && isTauri()
  const start = summaryStart(data, range)
  const summary = useQuery({
    queryKey: summaryKey(start, ticker, data),
    queryFn: () => getInvestmentSummary(start!, ticker),
    // The browser preview has the portfolio's summary from the synthetic seed.
    enabled: (native || !ticker) && Boolean(start),
    staleTime: SUMMARY_STALE_MS,
    retry: false,
    refetchInterval: (query) => (query.state.data?.refreshing ? REFRESHING_RECHECK_MS : false),
    // After a refresh the key changes; the same scope's last summary stays up, marked as
    // updating, for the moment Rust takes to answer, instead of a loading state.
    placeholderData: (previous, previousQuery) =>
      previousQuery?.queryKey[1] === start && previousQuery?.queryKey[2] === (ticker ?? '')
        ? previous
        : undefined,
  })
  const refreshing = Boolean(summary.data?.refreshing) || summary.isPlaceholderData
  return { start, summary, refreshing, current: refreshing ? undefined : summary.data }
}

/**
 * Builds the portfolio's summary for the page's period in the background, on launch, then has
 * Rust make each holding's, largest first, one at a time.
 */
export async function warmInvestmentSummary(
  client: ReturnType<typeof useQueryClient>,
  data: FinanceSnapshot,
  range: ChartPeriod,
) {
  const start = summaryStart(data, range)
  if (!start) return
  await client.fetchQuery({
    queryKey: summaryKey(start, undefined, data),
    queryFn: () => getInvestmentSummary(start),
    staleTime: SUMMARY_STALE_MS,
  })
  await warmInvestmentSummaries(start)
}

/**
 * Today in three parts: the market's wrap, how the investments moved and why the holdings
 * that led them moved, each reason quoted from a story, then what changed.
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
  const { start, summary, refreshing } = useInvestmentSummary(data, range, ticker)
  const result = summary.data
  const lines = result?.brief?.lines ?? []

  return (
    <Card className="portfolio-card investment-summary-card">
      <SectionHeading
        title="Summary"
        action={
          result ? (
            <>
              {refreshing ? (
                <span className="investment-summary-updating" role="status">
                  Updating
                </span>
              ) : null}
              <AsOf summary={result} />
            </>
          ) : undefined
        }
      />
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
          <MarketWrap summary={result} />
          <section className="brief-section" aria-label={ticker ?? 'You'}>
            {result.brief?.market ? <h3>{ticker ?? 'You'}</h3> : null}
            <p className="investment-brief">
              {lines.map((spans, index) => (
                <span key={index}>
                  {index ? ' ' : null}
                  <BriefLine spans={spans} />
                </span>
              ))}
            </p>
            <Headlines summary={result} />
          </section>
          <Changed summary={result} />
        </>
      ) : (
        <EmptyState>Today’s prices aren’t available yet.</EmptyState>
      )}
    </Card>
  )
}
