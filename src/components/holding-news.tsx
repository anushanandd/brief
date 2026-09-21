import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { RefreshCw } from 'lucide-react'

import { getMarketNews, isTauri } from '../lib/api'
import { averageNewsSentiment, rankHoldingNews } from '../lib/holding-news'
import { HoldingEarnings } from './holding-earnings'
import { NewsList } from './news-list'
import { Button, Card, EmptyState, SectionHeading } from './ui'

export function HoldingNews({ ticker, connected }: { ticker: string; connected: boolean }) {
  const native = typeof window !== 'undefined' && isTauri()
  const client = useQueryClient()
  const news = useQuery({
    queryKey: ['holding-news', 'alpha-vantage', ticker],
    queryFn: () => getMarketNews([ticker]),
    enabled: native && connected && Boolean(ticker),
    // Rust enforces daily freshness; this only rechecks saved status and budget.
    staleTime: 60 * 1000,
    refetchInterval: 60 * 1000,
    retry: false,
  })
  const refresh = useMutation({
    mutationFn: (symbol: string) => getMarketNews([symbol], true),
    onSuccess: (result, symbol) =>
      client.setQueryData(['holding-news', 'alpha-vantage', symbol], result),
  })
  const saved = news.data
  const error = refresh.variables === ticker && refresh.error ? refresh.error : news.error
  const warning = error ? (error instanceof Error ? error.message : error) : saved?.warning
  const groups = rankHoldingNews(saved?.articles ?? [], ticker, Date.now())
  const average = averageNewsSentiment(native && connected ? groups : [])
  const averageText =
    average.value == null
      ? '—'
      : new Intl.NumberFormat('en-US', {
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
          signDisplay: 'exceptZero',
        }).format(average.value)
  return (
    <Card className="holding-detail-card holding-news-card">
      <SectionHeading
        title="News"
        action={
          <span
            className="news-average"
            aria-label={`Average sentiment ${averageText}, across ${average.count} scored stories, excluding related duplicates`}
          >
            <span>Avg sentiment</span>
            <strong
              className={
                average.value != null && average.value > 0.15
                  ? 'positive'
                  : average.value != null && average.value < -0.15
                    ? 'negative'
                    : 'muted'
              }
            >
              {averageText}
            </strong>
          </span>
        }
      />
      <HoldingEarnings symbols={ticker ? [ticker] : []} connected={connected} />
      <div className="holding-news-content">
        {!ticker ? (
          <EmptyState>Select a holding to see related news.</EmptyState>
        ) : !native ? (
          <EmptyState>Alpha Vantage news is available in the native Brief app.</EmptyState>
        ) : !connected ? (
          <EmptyState>Connect Alpha Vantage in Settings for news and sentiment.</EmptyState>
        ) : news.isLoading ? (
          <EmptyState>Loading {ticker} news…</EmptyState>
        ) : (
          <>
            <div className="news-refresh-row">
              <span className="muted">
                {saved?.savedAt ? (
                  <>
                    Saved{' '}
                    <time dateTime={saved.savedAt}>
                      {new Date(saved.savedAt).toLocaleString([], {
                        month: 'short',
                        day: 'numeric',
                        hour: 'numeric',
                        minute: '2-digit',
                      })}
                    </time>
                  </>
                ) : (
                  'No saved news'
                )}
              </span>
              <Button
                icon={RefreshCw}
                aria-label="Refresh news (uses one API request)"
                disabled={news.isFetching || refresh.isPending || saved?.canRefresh === false}
                onClick={() => refresh.mutate(ticker)}
              >
                {refresh.isPending ? 'Refreshing…' : 'Refresh news'}
              </Button>
            </div>
            {saved && (
              <p className="news-budget muted">
                {saved.requestsRemaining} news requests left in Brief’s 24-hour budget. Five
                reserved for earnings and credential checks.
              </p>
            )}
            {warning && (
              <p className="news-warning" role="status">
                {warning}
              </p>
            )}
            {!warning && saved?.canRefresh === false && (
              <p className="news-warning" role="status">
                News refresh is paused by the request budget or provider rate limit. Saved stories
                remain available.
              </p>
            )}
            <NewsList
              articles={groups.map(({ article }) => article)}
              related={new Map(groups.map(({ article, related }) => [article.url, related]))}
              emptyMessage={
                warning
                  ? 'No saved stories available for this ticker.'
                  : `No recent stories matched ${ticker}.`
              }
            />
          </>
        )}
      </div>
    </Card>
  )
}
