import { Liveline, type LivelineHandle, type LivelinePoint } from 'liveline'
import { useEffect, useMemo, useRef, useState } from 'react'

import { asOfWaiting, useAsOf } from '../hooks/use-as-of'
import { useFinance } from '../hooks/use-finance'
import { useGraphScrollShortcuts } from '../hooks/use-graph-window-shortcuts'
import { useHoldingPrices } from '../hooks/use-holding-prices'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { addDays } from '../lib/dates'
import {
  formatInstant,
  formatMarketTime,
  formatPublicCurrency,
  formatSecurityName,
  formatUpdatedAt,
  pluralize,
} from '../lib/format'
import {
  type ChartPeriod,
  chartPeriodSeconds,
  useGraphEnd,
  useGraphWindow,
} from '../lib/graph-preferences'
import { type ConsolidatedPosition } from '../lib/holding-detail'
import { holdingNewsDigest, rankHoldingNews } from '../lib/holding-news'
import {
  holdingChartPoints,
  holdingChartCandles,
  holdingChartTimeline,
  visibleHoldingChartPoints,
  holdingPeriodChange,
  holdingRangeChange,
  holdingChartEnd,
  holdingChartTime,
  holdingWindowCovered,
} from '../lib/holding-prices'
import { holdingHistoryRange, rangeDates } from '../lib/investment-range'
import { summaryStart } from '../lib/investment-summary'
import { chartDayTime, firstChartDay } from '../lib/live-chart'
import { type HoldingPriceHistory } from '../lib/schema'
import {
  ChartReadout,
  type ChartTimeline,
  ChartSpan,
  ExpandableChart,
  useChartColors,
  useChartPan,
} from './charts'
import { useHoldingNews } from './holding-news'
import { Info } from './icons'
import { InvestmentRangeSelect } from './investments-summary'
import { SecurityMark } from './security-mark'
import { AnimatedCurrency, Button, Card, ChartChange, EmptyState } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

// A candle chart draws its candles, never a line.
const noPoints: LivelinePoint[] = []

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
  /** Each security once, its accounts' positions consolidated. */
  holdings: readonly Pick<ConsolidatedPosition, 'ticker' | 'name' | 'price'>[]
  ticker: string
  range: ChartPeriod
}) {
  const finance = useFinance()
  const colors = useChartColors()
  const tickerKey = [...new Set(holdings.map((holding) => holding.ticker))].join(',')
  const tickers = useMemo(() => tickerKey.split(','), [tickerKey])
  const [graphEnd] = useGraphEnd()
  const [, setGraphWindow] = useGraphWindow()
  const span = chartPeriodSeconds(period)
  // The native history to request, and the span of it on screen (three months and this year
  // crop the year's daily bars). Scrolled back in time, the history today's chart read stays
  // while it covers the window; past it, the whole daily history is read.
  const todayRange = holdingHistoryRange(period)
  const todayRead = useRef<{ history: HoldingPriceHistory; points: LivelinePoint[] }>(undefined)
  const read = todayRead.current
  const covered =
    graphEnd &&
    read?.history.symbol === ticker &&
    read.history.range === todayRange &&
    holdingWindowCovered(read.history, read.points, chartDayTime(graphEnd) + 43_199, span)
  const range = graphEnd && !covered ? 0 : todayRange
  const supported = finance.marketSymbols.includes(ticker)
  const prices = useHoldingPrices(ticker, range, finance.liveMarketEnabled && supported, tickers)
  const holding = holdings.find((position) => position.ticker === ticker)
  const history = prices.history
  // A chart scrolled back ends with its end day's last second (UTC), so today's quote is no
  // part of it.
  const quote = graphEnd ? undefined : prices.quote
  const liveNow = history?.cached ? history.fetchedAt / 1000 : prices.now
  const now = graphEnd ? chartDayTime(graphEnd) + 43_199 : liveNow
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
  useEffect(() => {
    todayRead.current =
      history?.range === todayRange && !prices.stale ? { history, points: allPoints } : undefined
  })
  const chartEnd = holdingChartEnd(prices, allPoints, now)
  // Where today's chart ends, scrolled back or not: a closed market's ends on its last trade.
  const latestEnd = holdingChartEnd(prices, allPoints, liveNow)
  const liveEnd =
    prices.connection?.status === 'closed'
      ? (allPoints.findLast((point) => point.time <= latestEnd)?.time ?? latestEnd)
      : latestEnd
  const points = useMemo(
    () => visibleHoldingChartPoints(allPoints, span, chartEnd, history),
    [allPoints, span, chartEnd, history],
  )
  const last = points.at(-1)
  const tradedSince = Boolean(quote && quote.time >= (last?.time ?? 0))
  const price = useSettledPrice(
    quote && tradedSince
      ? quote.price
      : (last?.value ?? (!finance.liveMarketEnabled ? (holding?.price ?? undefined) : undefined)),
    // A new end day's close shows at once, as the readout that followed the scroll there did.
    `${ticker}:${graphEnd ?? ''}`,
  )
  const timelineEnd = graphEnd ? chartEnd : liveEnd
  const windowTimeline = useMemo(
    () => holdingChartTimeline(history, allPoints, timelineEnd, span),
    [history, allPoints, timelineEnd, span],
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
  // Every candle in the history, so the chart can slide to any window without a render.
  const displayTime = windowTimeline.toDisplay
  const windowCandles = useMemo(
    () =>
      holdingChartCandles(
        { history, bars: prices.bars },
        { toDisplay: displayTime },
        Infinity,
        span,
      ),
    [history, prices.bars, displayTime, span],
  )
  // While a new range loads, the history read last stays up where the chart is; with none of it
  // there, the last chart drawn does.
  const view = {
    timeline: windowTimeline,
    candleChart: windowCandles,
    last: last ?? allPoints.at(-1),
    calendar: history?.calendar,
  }
  const drawn = useRef<{ ticker: string; view: typeof view }>(undefined)
  const fresh = points.length >= 2 || (prices.stale && allPoints.length >= 2)
  useEffect(() => {
    if (fresh) drawn.current = { ticker, view }
  })
  const shown = fresh
    ? view
    : prices.stale && drawn.current?.ticker === ticker
      ? drawn.current.view
      : undefined
  const { timeline, candleChart, last: drawnLast, calendar } = shown ?? view
  const candleLabel =
    windowCandles.resolution < 86400
      ? `${windowCandles.resolution / 60}m candles`
      : `${windowCandles.resolution / 86400}-day candles`
  // Measured as the summary beside it measures the period, from the same start; the trailing
  // span is the fallback when that start's close isn't in the history.
  const asOf = useAsOf(finance.data)
  const startDay = asOf ? summaryStart(asOf, period) : undefined
  // Measured to the latest point by now, so the clock's tick alone doesn't measure it again.
  const latestTime = allPoints.findLast((point) => point.time <= now)?.time
  const { historyError, error } = prices
  const periodChange = useMemo(
    () =>
      startDay && latestTime !== undefined
        ? holdingPeriodChange(
            { history, historyError, error, bars: [] },
            allPoints,
            latestTime,
            startDay,
          )
        : undefined,
    [history, historyError, error, allPoints, latestTime, startDay],
  )
  const change = periodChange ?? holdingRangeChange(prices, allPoints, now, span)
  // What the chart drawn to `at` reads, as it reads an end day once settled there: the close of
  // its last candle, and the change to it from the window's start.
  const priceAt = (at: number) => {
    const end = holdingChartEnd(prices, allPoints, at)
    return visibleHoldingChartPoints(allPoints, span, end, history).at(-1)?.value
  }
  const changeAt = (at: number) => {
    const from = rangeDates(period, new Date(at * 1000).toISOString().slice(0, 10)).from
    const latest = allPoints.findLast((point) => point.time <= at)?.time
    const moved =
      from && latest !== undefined
        ? holdingPeriodChange(
            { history, historyError, error, bars: [] },
            allPoints,
            latest,
            addDays(from, -1),
          )
        : undefined
    return {
      change: moved ?? holdingRangeChange(prices, allPoints, at, span),
      extended: moved?.extended,
    }
  }
  // Scrolled back, the readouts keep reading the chart until the day's history and figures are in.
  const waitingAt = graphEnd && (prices.stale || asOfWaiting(asOf)) ? now : undefined
  const connection = prices.connection
  const active = !graphEnd && prices.visible && connection?.status === 'subscribed' && !prices.error
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
  const chartVisible = supported && finance.liveMarketEnabled && Boolean(shown)
  const historyStatus = <LoadingText key={selection}>Loading price history…</LoadingText>
  const changeRow = (
    moved: typeof change,
    extended: NonNullable<typeof periodChange>['extended'],
    still = false,
  ) =>
    moved ? (
      <>
        <ChartChange
          still={still}
          amount={moved.change}
          percent={moved.percent}
          ariaLabel={`${savedChange ? 'Saved bar-close change' : 'Bar-close change'}${
            prices.savedComparison
              ? `, verified ${formatUpdatedAt(prices.savedComparison.asOf)}`
              : ''
          }`}
        />
        {extended ? (
          <span className="holding-chart-extended">
            {extended.label}
            <ChartChange
              still={still}
              amount={extended.change}
              percent={extended.percent}
              ariaLabel={`${extended.label} change since the close`}
            />
          </span>
        ) : null}
      </>
    ) : loadingHistory ? (
      <span className="holding-chart-status">{historyStatus}</span>
    ) : (
      <ChartChange amount={null} percent={null} />
    )
  const today = finance.data?.insights?.today
  const firstBar = history?.range === 0 ? firstChartDay(allPoints) : undefined
  const scroll = useMemo(
    () =>
      today && supported && finance.liveMarketEnabled
        ? // shortcut: a short range's history doesn't reach the security's first day, so until
          // the whole history loads any earlier day may be asked for; the next move clamps to it.
          { today, first: firstBar ?? addDays(today, -36_500) }
        : undefined,
    [today, firstBar, supported, finance.liveMarketEnabled],
  )
  useGraphScrollShortcuts(scroll)
  // The chart slides along its sessions, closures removed, and settles on the nearest one.
  const sessions = useMemo((): ChartTimeline | undefined => {
    if (!today) return undefined
    const { toDisplay, toActual } = timeline
    const end = (day: string | undefined) => (day ? chartDayTime(day) + 43_199 : liveEnd)
    const at = (day: string | undefined) => toDisplay(end(day))
    const days = (calendar ?? []).map(({ date }) => date).filter((date) => date < today)
    return {
      span: timeline.span * 0.95,
      toDisplay,
      toActual,
      end,
      day: (time) => {
        const target = toDisplay(time)
        // Before the sessions read, by calendar days.
        const first = days[0] ?? today
        if (target < at(first)) return addDays(first, Math.round((target - at(first)) / 86_400))
        let nearest: string | undefined
        for (const day of days)
          if (Math.abs(at(day) - target) < Math.abs(at(nearest) - target)) nearest = day
        return nearest
      },
    }
  }, [timeline, calendar, liveEnd, today])
  const [viewport, setViewport] = useState<HTMLDivElement | null>(null)
  const [expandedViewport, setExpandedViewport] = useState<HTMLDivElement | null>(null)
  const chart = useRef<LivelineHandle>(null)
  const expandedChart = useRef<LivelineHandle>(null)
  const panning = useChartPan(viewport, span, scroll, { chart, timeline: sessions })
  const expandedPanning = useChartPan(expandedViewport, span, scroll, {
    chart: expandedChart,
    timeline: sessions,
  })
  return (
    <Card className="net-worth-card brokerage-performance-card holding-chart-card">
      <ExpandableChart
        title={holding ? `${ticker} · ${formatSecurityName(holding.name)}` : 'Security price'}
        seconds={span}
        setSeconds={setGraphWindow}
        scroll={scroll}
      >
        {(expanded, expand) => (
          <>
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
                  <ChartReadout
                    waitingAt={waitingAt}
                    read={(at) => (
                      <AnimatedCurrency
                        still
                        className="hero-number"
                        value={priceAt(at)}
                        publicValue
                      />
                    )}
                  >
                    <AnimatedCurrency
                      key={ticker}
                      className="hero-number"
                      value={price}
                      publicValue
                    />
                  </ChartReadout>
                </div>
              </div>
              <div className="holding-chart-controls">
                <div className="holding-chart-actions">
                  <ChartSpan seconds={span}>
                    <InvestmentRangeSelect />
                  </ChartSpan>
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
                        <p>Last bar {formatUpdatedAt(last.time * 1000)}</p>
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
                <ChartReadout
                  waitingAt={waitingAt}
                  read={(at) => {
                    const moved = changeAt(at)
                    return changeRow(moved.change, moved.extended, true)
                  }}
                >
                  {changeRow(change, periodChange?.extended)}
                </ChartReadout>
              </div>
            </header>
            <div
              ref={expanded ? setExpandedViewport : setViewport}
              className={`brokerage-chart-viewport${expand ? ' is-expandable' : ''}`}
              role="group"
              aria-label={`${ticker} price history${newsMarkers.length ? ` with ${newsMarkers.length} top ${pluralize(newsMarkers.length, 'story', 'stories')} marked` : ''}`}
              aria-keyshortcuts={
                `${scroll ? '[ ] }' : ''}${expand ? ' Enter' : ''}`.trim() || undefined
              }
              tabIndex={0}
              data-range-focus-region
              onClick={(event) => {
                if (event.target instanceof HTMLCanvasElement) expand?.()
              }}
              onKeyDown={(event) => {
                if (event.key !== 'Enter' || event.target !== event.currentTarget || !expand) return
                event.preventDefault()
                expand()
              }}
            >
              {chartVisible ? (
                <div className="chart-container performance-chart live-performance-chart">
                  <div
                    className={`holding-session-overlay${(expanded ? expandedPanning : panning) ? ' is-moving' : ''}`}
                    aria-label="Trading sessions in Eastern time"
                  >
                    {timeline.sessions.map((session) => {
                      const left = Math.max(0, timeline.position(session.start))
                      const right = Math.min(95, timeline.position(session.end))
                      const label =
                        session.kind === 'regular' || session.label === 'Close'
                          ? `${session.label} ${formatMarketTime(session.start * 1000)}`
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
                    ref={expanded ? expandedChart : chart}
                    className="liveline-chart-canvas"
                    key={`${ticker}:${period}`}
                    mode="candle"
                    candles={candleChart.candles}
                    candleWidth={candleChart.width}
                    data={noPoints}
                    value={drawnLast!.value}
                    // Daily candles end on the last close; a trade since badges the price above.
                    badgeValue={tradedSince && candleChart.resolution >= 86_400 ? price : undefined}
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
                    minValueRange={drawnLast!.value * 0.001}
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
                              label: [ticker, latestLabel, formatPublicCurrency(quote.price)]
                                .filter(Boolean)
                                .join(' · '),
                            },
                          ]
                        : []),
                    ]}
                    scrub
                    formatValue={formatPublicCurrency}
                    formatTime={(time) =>
                      // Eastern, like the session marks; the crosshair has no room to say so.
                      range === 86400
                        ? formatInstant(timeline.toActual(time) * 1000, {
                            hour: 'numeric',
                            minute: '2-digit',
                            timeZone: 'America/New_York',
                          })
                        : formatInstant(timeline.toActual(time) * 1000, {
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
          </>
        )}
      </ExpandableChart>
    </Card>
  )
}
