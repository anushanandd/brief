import type { MarketNewsArticle } from '../lib/schema'
import { ExternalLink } from './external-link'
import { ArrowUpRight } from './icons'
import { EmptyState } from './ui'

const newsDate = new Intl.DateTimeFormat('en-US', {
  month: 'short',
  day: 'numeric',
  hour: 'numeric',
  minute: '2-digit',
})

export function NewsList({
  articles,
  emptyMessage,
  related,
}: {
  articles: MarketNewsArticle[]
  emptyMessage: string
  related?: Map<string, MarketNewsArticle[]>
}) {
  if (!articles.length) return <EmptyState>{emptyMessage}</EmptyState>
  return (
    <div className="news-list" role="region" aria-label="News" tabIndex={0} data-keyboard-region>
      {articles.map((article) => (
        <div className="news-story" key={article.url}>
          <ExternalLink
            className="news-row"
            data-keyboard-row
            data-keyboard-open
            href={article.url}
          >
            <span className="news-row-copy">
              <span className="news-row-meta">
                <span>{article.source}</span>
                <span aria-hidden="true">·</span>
                <time dateTime={article.createdAt}>
                  {newsDate.format(new Date(article.createdAt))}
                </time>
              </span>
              <strong>{article.headline}</strong>
            </span>
            <ArrowUpRight size={15} aria-hidden="true" />
          </ExternalLink>
          <div className="news-provider-scores">
            <span>
              Relevance ·{' '}
              {article.relevanceScore == null
                ? '—'
                : `${Math.round(article.relevanceScore * 100)}%`}
            </span>
            <span
              className={
                article.sentimentLabel?.includes('Bullish')
                  ? 'positive'
                  : article.sentimentLabel?.includes('Bearish')
                    ? 'negative'
                    : 'muted'
              }
            >
              Sentiment · {article.sentimentLabel?.replaceAll('-', ' ') ?? 'Unavailable'}
              {article.sentimentScore == null
                ? ''
                : ` · ${article.sentimentScore > 0 ? '+' : ''}${article.sentimentScore.toFixed(2)}`}
            </span>
          </div>
          {related?.get(article.url)?.length ? (
            <details className="news-related">
              <summary>
                {related.get(article.url)!.length} more{' '}
                {related.get(article.url)!.length === 1 ? 'article' : 'articles'}
              </summary>
              {related.get(article.url)!.map((other) => (
                <ExternalLink key={other.url} href={other.url}>
                  {other.source} · {other.headline}
                </ExternalLink>
              ))}
            </details>
          ) : null}
        </div>
      ))}
    </div>
  )
}
