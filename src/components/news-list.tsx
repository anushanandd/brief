import { pluralize } from '../lib/format'
import type { MarketNewsArticle, NewsTag, SecFiling } from '../lib/schema'
import { ExternalLink } from './external-link'
import { ArrowUpRight } from './icons'
import { Relevance } from './relevance'
import { EmptyState } from './ui'

const newsDate = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  hour: 'numeric',
  minute: '2-digit',
})

export const newsTagName: Record<NewsTag, string> = {
  earnings: 'Earnings',
  guidance: 'Guidance',
  product: 'Product',
  deal: 'Deal',
  legal: 'Legal',
  leadership: 'Leadership',
  capital: 'Dividends & buybacks',
  explainer: 'Why it moved',
}

function StoryRow({
  article,
  related,
}: {
  article: MarketNewsArticle
  related: MarketNewsArticle[] | undefined
}) {
  return (
    <div className="news-story">
      <ExternalLink className="news-row" data-keyboard-row data-keyboard-open href={article.url}>
        <span className="news-row-copy">
          <span className="news-row-meta">
            <Relevance value={article.relevance} />
            <span>{article.source}</span>
            <span aria-hidden="true">·</span>
            <time dateTime={article.createdAt}>{newsDate.format(new Date(article.createdAt))}</time>
          </span>
          <strong>{article.headline}</strong>
        </span>
        <ArrowUpRight size={15} aria-hidden="true" />
      </ExternalLink>
      {article.tags.length ? (
        <div className="news-tags" aria-label="Events">
          {article.tags.map((tag) => (
            <span key={tag}>{newsTagName[tag]}</span>
          ))}
        </div>
      ) : null}
      {related?.length ? (
        <details className="news-related">
          <summary>
            {related.length} more {pluralize(related.length, 'article')}
          </summary>
          {related.map((other) => (
            <ExternalLink key={other.url} href={other.url}>
              {other.source} · {other.headline}
            </ExternalLink>
          ))}
        </details>
      ) : null}
    </div>
  )
}

function FilingRow({ filing }: { filing: SecFiling }) {
  return (
    <div className="news-story">
      <ExternalLink className="news-row" data-keyboard-row data-keyboard-open href={filing.url}>
        <span className="news-row-copy">
          <span className="news-row-meta">
            <span>{`SEC ${filing.form}`}</span>
            <span aria-hidden="true">·</span>
            <time dateTime={filing.filedAt}>{newsDate.format(new Date(filing.filedAt))}</time>
          </span>
          <strong>{filing.title}</strong>
        </span>
        <ArrowUpRight size={15} aria-hidden="true" />
      </ExternalLink>
      <div className="news-tags" aria-label="Events">
        <span>SEC filing</span>
      </div>
    </div>
  )
}

/** Stories and, when given, SEC filings in one list, newest first. */
export function NewsList({
  articles,
  emptyMessage,
  related,
  filings = [],
}: {
  articles: MarketNewsArticle[]
  emptyMessage: string
  related?: Map<string, MarketNewsArticle[]>
  filings?: SecFiling[]
}) {
  if (!articles.length && !filings.length) return <EmptyState>{emptyMessage}</EmptyState>
  const rows = [
    ...articles.map((article) => ({ kind: 'story' as const, at: article.createdAt, article })),
    ...filings.map((filing) => ({ kind: 'filing' as const, at: filing.filedAt, filing })),
  ].toSorted((left, right) => Date.parse(right.at) - Date.parse(left.at))
  return (
    <div className="news-list" role="region" aria-label="News" tabIndex={0} data-keyboard-region>
      {rows.map((row) =>
        row.kind === 'story' ? (
          <StoryRow
            key={row.article.url}
            article={row.article}
            related={related?.get(row.article.url)}
          />
        ) : (
          <FilingRow key={row.filing.url} filing={row.filing} />
        ),
      )}
    </div>
  )
}
