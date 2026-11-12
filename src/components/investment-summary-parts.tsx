import { formatInstant, formatMarketTime, formatShortDay } from '../lib/format'
import type { BriefHeadline, InvestmentSummary } from '../lib/schema'
import { BriefLine } from './brief-line'
import { ExternalLink } from './external-link'

/**
 * A market time, Eastern like the session it belongs to, naming its day when it isn't the
 * session's: "4:00 PM ET", "Aug 28, 4:00 PM ET".
 */
const clock = (value: string, today?: string) => {
  const time = formatMarketTime(value)
  return today && value.slice(0, 10) !== today
    ? `${formatInstant(value, { month: 'short', day: 'numeric', timeZone: 'America/New_York' })}, ${time}`
    : time
}

/** When the prices behind the summary are from. */
export function AsOf({ summary }: { summary: InvestmentSummary }) {
  if (!summary.pricesAsOf) return null
  return (
    <small className="investment-summary-as-of">
      As of{' '}
      <time dateTime={summary.pricesAsOf}>{clock(summary.pricesAsOf, summary.session.today)}</time>
    </small>
  )
}

/** Who chose a quote, for its title: a sentence a reader picked from the story's text. */
const pickedBy = (quotedBy: string | null) =>
  quotedBy === 'apple'
    ? 'Sentence picked from the story by Apple Intelligence'
    : quotedBy?.startsWith('local:')
      ? `Sentence picked from the story by ${quotedBy.slice(6)} on this Mac`
      : quotedBy === 'rule'
        ? 'Sentence picked from the story by Brief’s rule'
        : undefined

/**
 * One story behind the day. A sentence picked from the story's text leads, quoted as written;
 * otherwise the headline leads with the publisher's summary under it.
 */
function Story({ headline, today }: { headline: BriefHeadline; today: string }) {
  const picked = headline.quote != null && headline.quotedBy !== 'summary'
  const ticker = headline.tickers[0]
  return (
    <ExternalLink href={headline.url} data-keyboard-row data-keyboard-open>
      {ticker ? <strong className="brief-story-ticker">{ticker}</strong> : null}
      <span className="brief-story-copy">
        <span title={picked ? pickedBy(headline.quotedBy) : undefined}>
          {picked ? `“${headline.quote}”` : headline.title}
        </span>
        {!picked && headline.quote ? (
          <span className="brief-story-detail">{headline.quote}</span>
        ) : null}
        <small>
          {headline.publisher} ·{' '}
          <time dateTime={headline.publishedAt}>{clock(headline.publishedAt, today)}</time>
        </small>
      </span>
    </ExternalLink>
  )
}

/** The market's wrap: the S&P 500's day as a reporter told it. */
export function MarketWrap({ summary }: { summary: InvestmentSummary }) {
  const market = summary.brief?.market
  if (!market) return null
  return (
    <section className="brief-section" aria-label="Market">
      <h3>Market</h3>
      <div className="brief-headlines">
        <Story headline={market} today={summary.session.today} />
      </div>
    </section>
  )
}

/**
 * Why the holdings that led the day moved: for each, the sentence from a story that says so,
 * linked, with its publisher and time.
 */
export function Headlines({ summary }: { summary: InvestmentSummary }) {
  const headlines = summary.brief?.headlines ?? []
  if (!headlines.length) return null
  const today = summary.session.today
  return (
    <ul className="brief-headlines" aria-label="Why they moved" data-keyboard-region>
      {headlines.map((headline) => (
        <li key={headline.url}>
          <Story headline={headline} today={today} />
        </li>
      ))}
    </ul>
  )
}

/**
 * What changed: earnings reported in the latest sessions, in Rust's words, then filings,
 * corporate actions, and company news, each with its source.
 */
export function Changed({ summary }: { summary: InvestmentSummary }) {
  const changed = summary.brief?.changed ?? []
  const earnings = summary.brief?.earnings ?? []
  if (!changed.length && !earnings.length) return null
  const today = summary.session.today
  return (
    <section className="brief-section" aria-label="Changed">
      <h3>Changed</h3>
      <ul className="brief-headlines" data-keyboard-region>
        {earnings.map((digest) => {
          const content = (
            <span className="brief-story-copy">
              <span>
                <BriefLine spans={digest.line} />
              </span>
              <small>
                Earnings ·{' '}
                <time dateTime={digest.reportedOn}>{formatShortDay(digest.reportedOn)}</time>
                {digest.guidance.length ? ' · gave guidance' : ''}
              </small>
            </span>
          )
          const href = digest.releaseUrl ?? digest.filingUrl
          return (
            <li key={`earnings:${digest.ticker}:${digest.reportedOn}`}>
              {href ? (
                <ExternalLink href={href} data-keyboard-row data-keyboard-open>
                  {content}
                </ExternalLink>
              ) : (
                <div className="brief-story-static">{content}</div>
              )}
            </li>
          )
        })}
        {changed.map((event) => {
          const at = event.publishedAt ?? event.at
          const content = (
            <>
              {event.ticker ? <strong className="brief-story-ticker">{event.ticker}</strong> : null}
              <span className="brief-story-copy">
                <span>{event.title}</span>
                <small>
                  {event.sourceLabel}
                  {at ? (
                    <>
                      {' · '}
                      <time dateTime={at}>{clock(at, today)}</time>
                    </>
                  ) : null}
                  {event.reaction ? ` · ${event.reaction}` : null}
                </small>
              </span>
            </>
          )
          return (
            <li key={event.id}>
              {event.url ? (
                <ExternalLink href={event.url} data-keyboard-row data-keyboard-open>
                  {content}
                </ExternalLink>
              ) : (
                <div className="brief-story-static">{content}</div>
              )}
            </li>
          )
        })}
      </ul>
    </section>
  )
}
