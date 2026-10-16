import { Liveline } from 'liveline'
import { useEffect, useMemo, useRef, useState } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useHoldingPrices } from '../hooks/use-holding-prices'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { formatCurrency, formatSecurityName, pluralize } from '../lib/format'
import { type ChartPeriod, chartPeriodSeconds } from '../lib/graph-preferences'
import { holdingNewsDigest, rankHoldingNews } from '../lib/holding-news'
import {
  holdingChartPoints,
  holdingChartCandles,
  holdingChartTimeline,
  visibleHoldingChartPoints,
  holdingRangeChange,
  holdingChartEnd,
  holdingChartTime,
} from '../lib/holding-prices'
import { holdingHistoryRange } from '../lib/investment-range'
import type { FinanceSnapshot } from '../lib/schema'
import { useChartColors } from './charts'
import { useHoldingNews } from './holding-news'
import { Info } from './icons'
import { InvestmentRangeSelect } from './investments-summary'
import { SecurityMark } from './security-mark'
import { AnimatedCurrency, Button, Card, ChartChange, EmptyState } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

function LoadingText({ children }: { children: string }) {
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    const timer = setTimeout(() => setVisible(true), 400)
    return () => clearTimeout(timer)
  }, [])
  return <span style={{ visibility: visible ? 'visible' : 'hidden' }}>{children}</span>
}

// Live trades arrive several times a second. A price that rolls its digits on every one reads
// as flicker, so the figure settles on the latest price at most once a second.
const PRICE_SETTLE_MS = 1000
function useSettledPrice(price: number | undefined, ticker: string) {
  const [shown, setShown] = useState({ ticker, price })
  const changedAt = useRef(0)
  useEffect(() => {
    const timer = setTimeout(
      () => {
        changedAt.current = Date.now()
        setShown({ ticker, price })
      },
      Math.max(0, changedAt.current + PRICE_SETTLE_MS - Date.now()),
    )
    return () => clearTimeout(timer)
  }, [ticker, price])
  // A new holding, or a first price, shows at once.
  return shown.ticker === ticker && shown.price != null ? shown.price : price
}

export function HoldingChart({
  holdings,
  ticker,
  range: period,
}: {
  holdings: FinanceSnapshot['holdings']
  ticker: string
  range: ChartPeriod
}) {
  const finance = useFinance()
  const colors = useChartColors()
  const tickerKey = [...new Set(holdings.map((holding) => holding.ticker))].join(',')
  const tickers = useMemo(() => tickerKey.split(','), [tickerKey])
  // The native history to request, and the span of it on screen (three months and this year
  // crop the year's daily bars).
  const range = holdingHistoryRange(period)
  const supported = finance.marketSymbols.includes(ticker)
  const prices = useHoldingPrices(ticker, range, finance.liveMarketEnabled && supported, tickers)
  const holding = holdings.find((position) => position.ticker === ticker)
  const quote = prices.quote
  const history = prices.history
  const now = history?.cached ? history.fetchedAt / 1000 : prices.now
  const allPoints = useMemo(
    () =>
      holdingChartPoints({
        history,
        bars: prices.bars,
        historyError: prices.historyError,
        error: prices.error,
      }),
    [history, prices.bars, prices.historyError, prices.error],
  )
  const chartEnd = holdingChartEnd(prices, allPoints, now)
  const span = chartPeriodSeconds(period)
  const points = useMemo(
    () => visibleHoldingChartPoints(allPoints, span, chartEnd),
    [allPoints, span, chartEnd],
  )
  const last = points.at(-1)
  const price = useSettledPrice(
    quote && quote.time >= (last?.time ?? 0)
      ? quote.price
      : (last?.value ?? (!finance.liveMarketEnabled ? (holding?.price ?? undefined) : undefined)),
    ticker,
  )
  const timelineEnd = prices.connection?.status === 'closed' ? (last?.time ?? chartEnd) : chartEnd
  const timeline = useMemo(
    () => holdingChartTimeline(history, points, timelineEnd, span),
    [history, points, timelineEnd, span],
  )
  const news = useHoldingNews(ticker)
  const [detailsOpen, setDetailsOpen] = useState(false)
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'holding.chart.details',
          title: 'Show price chart details',
          search: 'holding price chart feed candles market',
          group: 'View',
          scope: 'page',
          run: () => setDetailsOpen((open) => !open),
        },
      ],
      [],
    ),
  )
  // Where the top stories landed on the visible price history.
  const newsMarkers = useMemo(() => {
    const first = points[0]
    const latest = points.at(-1)
    if (!first || !latest) return []
    const groups = rankHoldingNews(news.data?.articles ?? [], ticker)
    return holdingNewsDigest(groups, latest.time * 1000).top.flatMap(({ article }) => {
      const time = Date.parse(article.createdAt) / 1000
      if (time < first.time || time > latest.time) return []
      // Sit on the price point the story landed after, so the marker stays on the line.
      const point = points.findLast((candidate) => candidate.time <= time) ?? first
      return [
        {
          id: `news:${article.url}`,
          time: point.time,
          value: point.value,
          headline: article.headline,
        },
      ]
    })
  }, [news.data, points, ticker])
  const candleChart = useMemo(
    () => holdingChartCandles({ history, bars: prices.bars }, timeline, chartEnd, span),
    [history, prices.bars, timeline, chartEnd, span],
  )
  const candleLabel =
    candleChart.resolution < 86400
      ? `${candleChart.resolution / 60}m candles`
      : `${candleChart.resolution / 86400}-day candles`
  const change = holdingRangeChange(prices, allPoints, now, span)
  const connection = prices.connection
  const active = prices.visible && connection?.status === 'subscribed' && !prices.error
  const stale = quote && now - quote.time > (connection?.delayMinutes ?? 0) * 60 + 120
  const feed =
    connection?.feed === 'overnight'
      ? 'Indicative'
      : connection?.feed === 'boats'
        ? 'BOATS'
        : connection?.feed === 'delayed_sip'
          ? 'SIP · 15m delayed'
          : 'SIP'
  const status = !holding
    ? 'No holdings available.'
    : !supported
      ? 'Live pricing is unavailable for this security.'
      : !finance.liveMarketEnabled
        ? 'Connect Alpaca in Settings for live prices.'
        : (prices.error ??
          (connection
            ? `${connection.session}${connection.feed ? ` · ${feed}` : ''} · ${!prices.visible ? 'Paused' : active ? (stale ? 'Older quote' : quote ? (connection.delayMinutes ? 'Streaming' : 'Live') : 'Waiting for quotes') : connection.status === 'rest' ? 'REST updates' : connection.status}`
            : null))
  const latestLabel = quote?.indicative ? undefined : 'Latest trade'
  const loadingHistory =
    supported && finance.liveMarketEnabled && !history && !prices.historyError && !prices.error
  const selection = `${ticker}:${period}`
  const savedChange = Boolean(history?.cached || prices.historyError || prices.error)
  const marketStatus = status ?? <LoadingText key={selection}>Loading market data…</LoadingText>
  const chartVisible = supported && finance.liveMarketEnabled && points.length >= 2
  const historyStatus = <LoadingText key={selection}>Loading price history…</LoadingText>
  return (
    <Card className="net-worth-card brokerage-performance-card holding-chart-card">
      <header className="home-balance-header">
        <div className="home-balance-main">
          <h2 className="balance-label holding-chart-title">
            {holding ? (
              <>
                <SecurityMark ticker={ticker} name={holding.name} small />
                {`${ticker} · ${formatSecurityName(holding.name)}`}
              </>
            ) : (
              'Security price'
            )}
          </h2>
          <div className="home-balance-value">
            <AnimatedCurrency key={ticker} className="hero-number" value={price} />
          </div>
        </div>
        <div className="holding-chart-controls">
          <div className="holding-chart-actions">
            <InvestmentRangeSelect />
            <Tooltip open={detailsOpen} onOpenChange={setDetailsOpen}>
              <TooltipTrigger
                render={
                  <Button
                    type="button"
                    size="icon-compact"
                    variant="ghost"
                    className="icon-only-subtle"
                    aria-label="Price chart details"
                    data-command="holding.chart.details"
                  />
                }
              >
                <Info size={16} aria-hidden="true" />
              </TooltipTrigger>
              <TooltipContent
                className="holding-chart-tooltip"
                side="bottom"
                align="end"
                sideOffset={6}
              >
                <p>
                  {history
                    ? `${candleLabel} · ${history.feeds.join(' + ').toUpperCase()}`
                    : loadingHistory
                      ? historyStatus
                      : (prices.historyError ?? 'Price history unavailable.')}
                </p>
                <p>
                  {marketStatus}
                  {quote ? (
                    <time dateTime={new Date(quote.time * 1000).toISOString()}>
                      {` · ${holdingChartTime(quote.time, prices.now, true)}`}
                    </time>
                  ) : null}
                </p>
                {history?.cached ? (
                  <p>Saved history, verifying</p>
                ) : history && last && chartEnd !== now ? (
                  <p>Last bar {new Date(last.time * 1000).toLocaleString()}</p>
                ) : null}
              </TooltipContent>
            </Tooltip>
          </div>
          {chartVisible &&
          (!holding ||
            !supported ||
            !finance.liveMarketEnabled ||
            prices.error ||
            prices.historyError ||
            !prices.visible ||
            stale ||
            connection?.status === 'reconnecting' ||
            connection?.status === 'unavailable') ? (
            <span role="status" className="holding-chart-status">
              {prices.historyError ?? status}
            </span>
          ) : null}
        </div>
        <div className="chart-summary-row">
          {change ? (
            <ChartChange
              amount={change.change}
              percent={change.percent}
              ariaLabel={`${savedChange ? 'Saved bar-close change' : 'Bar-close change'}${
                prices.savedComparison
                  ? `, verified ${new Date(prices.savedComparison.asOf).toLocaleString()}`
                  : ''
              }`}
            />
          ) : loadingHistory ? (
            <span className="holding-chart-status">{historyStatus}</span>
          ) : (
            <ChartChange amount={null} percent={null} />
          )}
        </div>
      </header>
      <div
        className="brokerage-chart-viewport"
        role="group"
        aria-label={`${ticker} price history${newsMarkers.length ? ` with ${newsMarkers.length} top ${pluralize(newsMarkers.length, 'story', 'stories')} marked` : ''}`}
        tabIndex={0}
        data-range-focus-region
      >
        {chartVisible ? (
          <div className="chart-container performance-chart live-performance-chart">
            <div className="holding-session-overlay" aria-label="Trading sessions in Eastern time">
              {timeline.sessions.map((session) => {
                const left = Math.max(0, timeline.position(session.start))
                const right = Math.min(95, timeline.position(session.end))
                const label =
                  session.kind === 'regular' || session.label === 'Close'
                    ? `${session.label} ${new Date(session.start * 1000).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', timeZone: 'America/New_York' })} ET`
                    : session.label
                return right > left ? (
                  <div
                    key={session.start}
                    className={`holding-session holding-session-${session.kind}`}
                    style={{ left: `${left}%`, width: `${right - left}%` }}
                    aria-label={label}
                  >
                    {range === 86400 &&
                    right - left > 12 &&
                    timeline.position(session.start) >= 0 ? (
                      <span>{label}</span>
                    ) : null}
                  </div>
                ) : null
              })}
              {(timeline.gaps.length <= 24 ? timeline.gaps : []).map((gap) => (
                <div
                  key={gap.start}
                  className="holding-session-closure"
                  style={{ left: `${timeline.position(gap.end)}%` }}
                  aria-label="Closed session compressed"
                >
                  ⋮
                </div>
              ))}
            </div>
            <Liveline
              className="liveline-chart-canvas"
              key={`${ticker}:${period}`}
              mode="candle"
              candles={candleChart.candles}
              candleWidth={candleChart.width}
              data={timeline.points}
              value={last!.value}
              valueTime={timeline.toDisplay(last!.time)}
              endTime={timeline.end}
              theme={colors.theme}
              color={colors.value}
              window={timeline.span}
              padding={{ top: 24, right: 80, bottom: 28, left: 12 }}
              badge
              badgeVariant="minimal"
              currentLine={false}
              grid
              momentum={false}
              pulse={false}
              continuous={active}
              paused={false}
              fill
              lineWidth={1.75}
              minValueRange={last!.value * 0.001}
              referenceLine={quote ? { value: quote.price, label: latestLabel } : undefined}
              markers={[
                ...newsMarkers.map(({ id, time, value, headline }) => ({
                  id,
                  time: timeline.toDisplay(time),
                  value,
                  color: colors.benchmark,
                  label: `News · ${headline}`,
                })),
                ...(quote
                  ? [
                      {
                        id: 'latest',
                        time: timeline.toDisplay(quote.time),
                        value: quote.price,
                        color: colors.deposits,
                        label: [ticker, latestLabel, formatCurrency(quote.price)]
                          .filter(Boolean)
                          .join(' · '),
                      },
                    ]
                  : []),
              ]}
              scrub
              formatValue={formatCurrency}
              formatTime={(time) =>
                range === 86400
                  ? new Date(timeline.toActual(time) * 1000).toLocaleTimeString(undefined, {
                      hour: 'numeric',
                      minute: '2-digit',
                      timeZone: 'America/New_York',
                    })
                  : new Date(timeline.toActual(time) * 1000).toLocaleDateString(undefined, {
                      month: 'short',
                      day: 'numeric',
                      ...(span === 0 || span >= 365 * 86400 ? { year: 'numeric' } : {}),
                      ...(range === 604800 || range === 2592000
                        ? ({ hour: 'numeric', minute: '2-digit' } as const)
                        : {}),
                      timeZone: 'America/New_York',
                    })
              }
              style={{ flex: 1, minHeight: 0, height: 'auto' }}
            />
          </div>
        ) : (
          <EmptyState>
            {loadingHistory
              ? historyStatus
              : history
                ? 'No completed trades in this range.'
                : (prices.historyError ?? marketStatus)}
          </EmptyState>
        )}
      </div>
    </Card>
  )
}
