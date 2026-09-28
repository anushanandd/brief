import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'

import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { getMarketNews, isTauri } from '../lib/api'
import { rankHoldingNews } from '../lib/holding-news'
import { HoldingEarnings } from './holding-earnings'
import { RefreshCw } from './icons'
import { NewsList } from './news-list'
import { Button, EmptyState, ScrollCueCard, SectionHeading } from './ui'

export function HoldingNews({ ticker, connected }: { ticker: string; connected: boolean }) {
  const native = typeof window !== 'undefined' && isTauri()
  const client = useQueryClient()
  const newsScrollRef = useViewportScroll(48)
  const news = useQuery({
    queryKey: ['holding-news', 'alpha-vantage', ticker],
    queryFn: () => getMarketNews([ticker]),
    enabled: native && Boolean(ticker),
    staleTime: Infinity,
    retry: false,
  })
  const refresh = useMutation({
    mutationFn: (symbol: string) => getMarketNews([symbol], true),
    onSuccess: (result, symbol) =>
      client.setQueryData(['holding-news', 'alpha-vantage', symbol], result),
  })
  const saved = news.data
  const refreshing = refresh.variables === ticker && refresh.isPending
  const error = refresh.variables === ticker && refresh.error ? refresh.error : news.error
  const warning = error ? (error instanceof Error ? error.message : error) : saved?.warning
  const groups = rankHoldingNews(saved?.articles ?? [], ticker)
  return (
    <ScrollCueCard
      className="holding-detail-card holding-news-card"
      scrollSelector=".holding-news-content"
    >
      <SectionHeading
        title="News"
        action={
          native && ticker ? (
            <Button
              icon={RefreshCw}
              size="icon-compact"
              variant="ghost"
              aria-label="Refresh news (uses one API request)"
              aria-busy={refreshing}
              disabled={!connected || news.isFetching || refreshing}
              onClick={() => refresh.mutate(ticker)}
            >
              {refreshing ? 'Refreshing news' : 'Refresh news'}
            </Button>
          ) : undefined
        }
      />
      <HoldingEarnings symbols={ticker ? [ticker] : []} connected={connected} />
      <div ref={newsScrollRef} className="holding-news-content">
        {!ticker ? (
          <EmptyState>Select a holding to see related news.</EmptyState>
        ) : !native ? (
          <EmptyState>Alpha Vantage news is available in the native Brief app.</EmptyState>
        ) : news.isLoading ? (
          <EmptyState>Loading {ticker} news…</EmptyState>
        ) : (
          <>
            {!connected && (
              <p className="news-warning" role="status">
                Connect Alpha Vantage in Settings to refresh. Saved stories remain available.
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
                !connected
                  ? 'Connect Alpha Vantage in Settings to load news for this ticker.'
                  : warning
                    ? 'No saved stories available for this ticker.'
                    : `No saved stories matched ${ticker}.`
              }
            />
          </>
        )}
      </div>
    </ScrollCueCard>
  )
}
