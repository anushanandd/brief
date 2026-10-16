import type { InvestmentSummary } from '../lib/schema'
import { ExternalLink } from './external-link'

/** A time of day, naming its day when it isn't the session's: "4:00 PM", "Aug 28, 4:00 PM". */
const clock = (value: string, today?: string) => {
  const date = new Date(value)
  const time = date.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })
  return today && value.slice(0, 10) !== today
    ? `${date.toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}, ${time}`
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

/**
 * The news that explains the day: each headline as published, linked, with its publisher and
 * time.
 */
export function Headlines({ summary }: { summary: InvestmentSummary }) {
  const headlines = summary.brief?.headlines ?? []
  if (!headlines.length) return null
  const today = summary.session.today
  return (
    <ul className="brief-headlines" aria-label="In the news" data-keyboard-region>
      {headlines.map((headline) => (
        <li key={headline.url}>
          <ExternalLink href={headline.url} data-keyboard-row data-keyboard-open>
            <span>{headline.title}</span>
            <small>
              {headline.publisher} ·{' '}
              <time dateTime={headline.publishedAt}>{clock(headline.publishedAt, today)}</time>
            </small>
          </ExternalLink>
        </li>
      ))}
    </ul>
  )
}
