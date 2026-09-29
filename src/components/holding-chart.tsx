import { Liveline } from 'liveline'
import { useCallback, useEffect, useMemo, useState } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useGraphAccountShortcuts } from '../hooks/use-graph-window-shortcuts'
import { useHoldingPrices } from '../hooks/use-holding-prices'
import { formatCurrency, formatSecurityName } from '../lib/format'
import {
  getDefaultHoldingChartRange,
  holdingChartPoints,
  holdingChartCandles,
  holdingChartTimeline,
  visibleHoldingChartPoints,
  holdingChartRanges,
  holdingRangeChange,
  holdingChartEnd,
  holdingChartTime,
} from '../lib/holding-prices'
import { pageShortcutBlocked } from '../lib/keyboard'
import type { FinanceSnapshot } from '../lib/schema'
import { useChartColors } from './charts'
import { Info } from './icons'
import { AnimatedCurrency, Button, Card, ChartChange, ChartRangeSelect, EmptyState } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

function LoadingText({ children }: { children: string }) {
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    const timer = setTimeout(() => setVisible(true), 400)
    return () => clearTimeout(timer)
  }, [])
  return <span style={{ visibility: visible ? 'visible' : 'hidden' }}>{children}</span>
}

export function HoldingChart({
  holdings,
  ticker,
  onTickerChange,
}: {
  holdings: FinanceSnapshot['holdings']
  ticker: string
  onTickerChange: (ticker: string) => void
}) {
  const finance = useFinance()
  const colors = useChartColors()
  const tickerKey = [...new Set(holdings.map((holding) => holding.ticker))].join(',')
  const tickers = useMemo(() => tickerKey.split(','), [tickerKey])
  const [range, setRange] = useState(getDefaultHoldingChartRange)
  const supported = finance.marketSymbols.includes(ticker)
  const prices = useHoldingPrices(ticker, range, finance.liveMarketEnabled && supported, tickers)
  const index = tickers.indexOf(ticker)
  const move = useCallback(
    (direction: -1 | 1) => {
      if (tickers.length)
        onTickerChange(tickers[(index + direction + tickers.length) % tickers.length])
    },
    [index, tickers, onTickerChange],
  )
  useGraphAccountShortcuts(move)
  const holding = holdings.find((position) => position.ticker === ticker)
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        event.shiftKey ||
        pageShortcutBlocked(event)
      )
        return
      const next = holdingChartRanges.find(
        ({ label }) => label.toLowerCase() === event.key.toLowerCase(),
      )
      if (next) {
        event.preventDefault()
        setRange(next.value)
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [])
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
  const points = useMemo(
    () => visibleHoldingChartPoints(allPoints, range, chartEnd),
    [allPoints, range, chartEnd],
  )
  const last = points.at(-1)
  const price =
    quote && quote.time >= (last?.time ?? 0)
      ? quote.price
      : (last?.value ?? (!finance.liveMarketEnabled ? holding?.price : undefined))
  const timelineEnd = prices.connection?.status === 'closed' ? (last?.time ?? chartEnd) : chartEnd
  const timeline = useMemo(
    () => holdingChartTimeline(history, points, timelineEnd, range),
    [history, points, timelineEnd, range],
  )
  const candleChart = useMemo(
    () => holdingChartCandles({ history, bars: prices.bars }, timeline, chartEnd, range),
    [history, prices.bars, timeline, chartEnd, range],
  )
  const candleLabel =
    candleChart.resolution < 86400
      ? `${candleChart.resolution / 60}m candles`
      : `${candleChart.resolution / 86400}-day candles`
  const change = holdingRangeChange(prices, allPoints, now)
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
  const selection = `${ticker}:${range}`
  const savedChange = Boolean(history?.cached || prices.historyError || prices.error)
  const marketStatus = status ?? <LoadingText key={selection}>Loading market data…</LoadingText>
  const historyStatus = <LoadingText key={selection}>Loading price history…</LoadingText>
  return (
    <Card className="net-worth-card brokerage-performance-card holding-chart-card">
      <header className="home-balance-header">
        <div className="home-balance-main">
          <h2 className="balance-label">
            {holding ? `${ticker} · ${formatSecurityName(holding.name)}` : 'Security price'}
          </h2>
          <div className="home-balance-value">
            <AnimatedCurrency key={ticker} className="hero-number" value={price} />
          </div>
        </div>
        <div className="holding-chart-controls">
          <div className="holding-chart-actions">
            <ChartRangeSelect
              label="Security price range"
              options={holdingChartRanges}
              value={range}
              onValueChange={setRange}
            />
            <Tooltip>
              <TooltipTrigger
                render={
                  <Button
                    type="button"
                    size="icon-compact"
                    variant="ghost"
                    className="icon-only-subtle"
                    aria-label="Price chart details"
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
                {history ? (
                  <>
                    <p>
                      {candleLabel} · {history.feeds.join(' + ').toUpperCase()}
                      {history.delayMinutes ? ` · ${history.delayMinutes}m delayed` : ''}
                    </p>
                    <p>
                      Last bar: {last ? new Date(last.time * 1000).toLocaleString() : 'Unavailable'}
                      {chartEnd !== now ? ' · Last observed day' : ''}
                    </p>
                    <p>
                      Reported OHLC · split-adjusted · ET; closed sessions compressed.
                      {quote ? ` ${latestLabel ?? 'Latest quote'} is separate from bars.` : ''}
                    </p>
                    {history.cached ? <p>Saved history awaiting verification.</p> : null}
                    {!history.cached && savedChange ? (
                      <p>Last verified bar-close comparison.</p>
                    ) : null}
                  </>
                ) : (
                  <p>
                    {loadingHistory
                      ? historyStatus
                      : (prices.historyError ?? 'Price history unavailable.')}
                  </p>
                )}
                <p>
                  {marketStatus}
                  {quote ? (
                    <time dateTime={new Date(quote.time * 1000).toISOString()}>
                      {` · ${holdingChartTime(quote.time, prices.now, true)}`}
                    </time>
                  ) : null}
                </p>
              </TooltipContent>
            </Tooltip>
          </div>
          {!holding ||
          !supported ||
          !finance.liveMarketEnabled ||
          prices.error ||
          prices.historyError ||
          !prices.visible ||
          stale ||
          connection?.status === 'reconnecting' ||
          connection?.status === 'unavailable' ? (
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
          ) : (
            <span className="holding-chart-status">
              {loadingHistory ? historyStatus : 'Range change unavailable'}
            </span>
          )}
        </div>
      </header>
      <div className="brokerage-chart-viewport">
        {supported && finance.liveMarketEnabled && points.length >= 2 ? (
          <div
            className="chart-container performance-chart live-performance-chart"
            role="group"
            aria-label={`${ticker} price history`}
          >
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
              key={`${ticker}:${range}`}
              mode="candle"
              candles={candleChart.candles}
              candleWidth={candleChart.width}
              data={timeline.points}
              value={last!.value}
              valueTime={timeline.toDisplay(last!.time)}
              endTime={timeline.end}
              theme="dark"
              color={colors.value}
              window={timeline.window}
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
              markers={
                quote
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
                  : []
              }
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
                      ...(range === 0 || range >= 365 * 86400 ? { year: 'numeric' } : {}),
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
                : marketStatus}
          </EmptyState>
        )}
      </div>
    </Card>
  )
}
