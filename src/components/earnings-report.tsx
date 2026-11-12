import { formatPublicCurrency, formatShare, formatShortDay } from '../lib/format'
import type { ChartPeriod } from '../lib/graph-preferences'
import type { FinanceSnapshot, InvestmentSummary } from '../lib/schema'
import { BriefLine } from './brief-line'
import { ExternalLink } from './external-link'
import { useInvestmentSummary } from './investment-summary-card'
import { Card, EmptyState, SectionHeading } from './ui'

/** Who chose the guidance sentences, said plainly. */
const chosenBy = (by: string | null, from = 'the release’s sentences') =>
  by === 'apple'
    ? `Chosen by Apple Intelligence from ${from}`
    : by?.startsWith('local:')
      ? `Chosen by ${by.slice(6)} on this Mac from ${from}`
      : `Chosen by Brief’s rule from ${from}`

/**
 * A holding's latest earnings report: what it earned against the consensus and how the stock's
 * own move answered, in Rust's words, then the release's own sentences on revenue and guidance.
 */
export function EarningsReport({
  data,
  ticker,
  range,
}: {
  data: FinanceSnapshot
  ticker: string
  range: ChartPeriod
}) {
  // Only a current summary has the sessions a report's reaction is measured over.
  const { summary, current, refreshing } = useInvestmentSummary(data, range, ticker)
  const digest = current?.earnings[0]
  return (
    <Card className="holding-detail-card holding-earnings-card">
      <SectionHeading
        title="Earnings"
        action={
          digest ? (
            <small className="holding-earnings-when">
              {formatShortDay(digest.reportedOn)}
              {digest.reportTime === 'post-market'
                ? ' · after the close'
                : digest.reportTime === 'pre-market'
                  ? ' · before the open'
                  : ''}
            </small>
          ) : undefined
        }
      />
      {!digest ? (
        <EmptyState>
          {summary.isLoading || refreshing
            ? 'Reading reports…'
            : 'No earnings report in the last four months.'}
        </EmptyState>
      ) : (
        <div className="holding-earnings">
          <p className="holding-earnings-line">
            <BriefLine spans={digest.line} />
          </p>
          {digest.expectedMovePct != null ? (
            <small className="holding-earnings-expected">
              The options market priced in a move of {formatShare(digest.expectedMovePct)} either
              way.
            </small>
          ) : null}
          {digest.revenue ? (
            <div className="holding-earnings-quote">
              <small>Revenue</small>
              <q>{digest.revenue}</q>
            </div>
          ) : null}
          <div className="holding-earnings-quote">
            <small>Guidance</small>
            {digest.guidance.length ? (
              <ul title={chosenBy(digest.guidanceBy)}>
                {digest.guidance.map((sentence) => (
                  <li key={sentence}>
                    <q>{sentence}</q>
                  </li>
                ))}
              </ul>
            ) : (
              <span className="muted">
                {digest.releaseRead
                  ? 'The release gives no outlook; some companies give it only on the call.'
                  : 'The release hasn’t been read yet.'}
              </span>
            )}
          </div>
          {digest.callGuidance.length ? (
            <div className="holding-earnings-quote">
              <small>On the call</small>
              <ul title={chosenBy(digest.callGuidanceBy, 'management’s remarks on the call')}>
                {digest.callGuidance.map((sentence) => (
                  <li key={sentence}>
                    <q>{sentence}</q>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          <NextQuarter summary={current} />
          <div className="holding-earnings-links">
            {digest.releaseUrl ? (
              <ExternalLink href={digest.releaseUrl} data-keyboard-row data-keyboard-open>
                Press release
              </ExternalLink>
            ) : null}
            {digest.filingUrl ? (
              <ExternalLink href={digest.filingUrl} data-keyboard-row data-keyboard-open>
                8-K filing
              </ExternalLink>
            ) : null}
          </div>
        </div>
      )}
      {!digest && current ? <NextQuarter summary={current} /> : null}
    </Card>
  )
}

/**
 * What analysts expect of the next quarter and how that moved in a month, and the move the
 * options market prices in around the next report: other people's expectations, labeled.
 */
function NextQuarter({ summary }: { summary?: InvestmentSummary }) {
  const expectation = summary?.expectations.find((item) => item.ticker === summary.ticker)
  const quarter = expectation?.estimates.find((estimate) => estimate.horizon === 'fiscal quarter')
  if (!expectation || (!quarter && expectation.expectedMovePct == null)) return null
  return (
    <div className="holding-earnings-quote">
      <small>Expected</small>
      {quarter ? (
        <span>
          Analysts expect {formatPublicCurrency(quarter.eps)} a share for the quarter ending{' '}
          {formatShortDay(quarter.fiscalDateEnding)}
          {quarter.changePct != null
            ? `, ${quarter.changePct > 0 ? 'up' : quarter.changePct < 0 ? 'down' : 'unchanged'}${quarter.changePct ? ` ${formatShare(Math.abs(quarter.changePct))}` : ''} in 30 days`
            : ''}
          {quarter.revisionsUp != null
            ? ` (${quarter.revisionsUp} raised, ${quarter.revisionsDown ?? 0} cut)`
            : ''}
          .
        </span>
      ) : null}
      {expectation.nextReport && expectation.expectedMovePct != null ? (
        <span>
          The options market prices in {formatShare(expectation.expectedMovePct)} either way around
          the {formatShortDay(expectation.nextReport)} report.
        </span>
      ) : null}
    </div>
  )
}
