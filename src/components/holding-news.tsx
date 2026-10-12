import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useMemo } from 'react'

import { useViewportScroll } from '../hooks/use-viewport-scroll'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { getMarketNews, isTauri } from '../lib/api'
import { pluralize } from '../lib/format'
import {
  newsNeedsRefresh,
  newsRefreshMinutes,
  rankHoldingNews,
  RELEVANT,
} from '../lib/holding-news'
import type { MarketNewsResult } from '../lib/schema'
import { RefreshCw } from './icons'
import { NewsList } from './news-list'
import { Button, EmptyState, ScrollCueCard, SectionHeading } from './ui'

const newsKey = (ticker: string) => ['holding-news', 'alpaca', ticker]

/** Saved Alpaca stories and SEC filings for one ticker; the chart and the News card share it. */
export function useHoldingNews(ticker: string) {
  const native = typeof window !== 'undefined' && isTauri()
  return useQuery({
    queryKey: newsKey(ticker),
    queryFn: () => getMarketNews([ticker]),
    enabled: native && Boolean(ticker),
    staleTime: Infinity,
    retry: false,
  })
}

// When each ticker last refreshed on its own, so a failed or empty fetch waits before retrying.
const autoRefreshed = new Map<string, number>()

/**
 * A holding's news: stories about the company, newest first, with stories that only mention it
 * (broker notes on peers, roundups) folded away. Why it moved lives in the summary.
 */
export function HoldingNews({
  ticker,
  connected,
  secConnected,
}: {
  ticker: string
  connected: boolean
  secConnected: boolean
}) {
  const native = typeof window !== 'undefined' && isTauri()
  const client = useQueryClient()
  const newsScrollRef = useViewportScroll(48)
  const news = useHoldingNews(ticker)
  const save = (result: MarketNewsResult, symbol: string) => {
    client.setQueryData(newsKey(symbol), result)
    // The summary's events and sources come from the saved stories.
    void client.invalidateQueries({ queryKey: ['investment-summary'] })
  }
  const refresh = useMutation({
    mutationFn: (symbol: string) => getMarketNews([symbol], true),
    onSuccess: save,
  })
  const saved = news.data
  const { mutate: refreshNews, isPending: refreshPending } = refresh
  useEffect(() => {
    if (!native || !connected || !saved?.canRefresh || refreshPending) return
    const now = Date.now()
    const last = autoRefreshed.get(ticker)
    if (last != null && now - last < newsRefreshMinutes * 60_000) return
    // A saved SEC contact loads filings on the first visit, even when stories are fresh.
    const filingsMissing = secConnected && !saved.filingsSavedAt
    if (!filingsMissing && !newsNeedsRefresh(saved.savedAt, now)) return
    autoRefreshed.set(ticker, now)
    refreshNews(ticker)
  }, [native, connected, secConnected, saved, refreshPending, ticker, refreshNews])
  const refreshing = refresh.variables === ticker && refresh.isPending
  const canRefresh =
    native &&
    Boolean(ticker) &&
    connected &&
    !news.isFetching &&
    !refreshing &&
    saved?.canRefresh !== false
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        native && ticker
          ? [
              {
                id: 'holding.news.refresh',
                title: `Refresh ${ticker} news`,
                search: 'holding news stories filings refresh',
                group: 'Actions',
                scope: 'page',
                disabled: !canRefresh,
                run: () => refreshNews(ticker),
              },
            ]
          : [],
      [native, ticker, canRefresh, refreshNews],
    ),
  )
  const error = refresh.variables === ticker && refresh.error ? refresh.error : news.error
  const warning = error ? (error instanceof Error ? error.message : error) : saved?.warning
  const articles = saved?.articles ?? []
  const company = rankHoldingNews(
    articles.filter(({ scope, relevance }) => scope === 'company' && relevance >= RELEVANT),
    ticker,
  )
  // Stories that only mention the company, and market roundups, fold away together.
  const mentions = articles
    .filter(({ scope, relevance }) => scope === 'market' || relevance < RELEVANT)
    .toSorted(
      (left, right) =>
        right.relevance - left.relevance || right.createdAt.localeCompare(left.createdAt),
    )
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
              aria-label="Refresh news"
              aria-busy={refreshing}
              data-command="holding.news.refresh"
              disabled={!canRefresh}
              onClick={() => refresh.mutate(ticker)}
            >
              {refreshing ? 'Refreshing news' : 'Refresh news'}
            </Button>
          ) : undefined
        }
      />
      <div ref={newsScrollRef} className="holding-news-content">
        {!ticker ? (
          <EmptyState>Select a holding to see related news.</EmptyState>
        ) : !native ? (
          <EmptyState>News needs the desktop app.</EmptyState>
        ) : news.isLoading ? (
          <EmptyState>Loading {ticker} news…</EmptyState>
        ) : (
          <>
            {!connected && (
              <p className="news-warning" role="status">
                Connect Alpaca in Settings to refresh.
              </p>
            )}
            {warning && (
              <p className="news-warning" role="status">
                {warning}
              </p>
            )}
            {saved?.filingsWarning ? (
              <p className="news-warning" role="status">
                {`SEC EDGAR: ${saved.filingsWarning}`}
              </p>
            ) : null}
            <NewsList
              articles={company.map(({ article }) => article)}
              related={new Map(company.map(({ article, related }) => [article.url, related]))}
              filings={saved?.filings}
              emptyMessage={
                !connected
                  ? 'Connect Alpaca in Settings to load news.'
                  : warning
                    ? 'No saved stories available for this ticker.'
                    : `No recent stories about ${ticker}.`
              }
            />
            {mentions.length ? (
              <details className="news-market">
                <summary>
                  {`${mentions.length} other ${pluralize(mentions.length, 'mention')}`}
                </summary>
                <NewsList articles={mentions} emptyMessage="" />
              </details>
            ) : null}
          </>
        )}
      </div>
    </ScrollCueCard>
  )
}
