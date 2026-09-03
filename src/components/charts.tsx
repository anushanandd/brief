import { Liveline, type LivelinePoint, type LivelineSeries } from 'liveline'
import { useReducedMotion } from 'motion/react'
import { useTheme } from 'next-themes'
import { Pie, PieChart, Sector, Tooltip, type PieSectorShapeProps } from 'recharts'

import { formatCompactCurrency, formatCurrency } from '../lib/format'
import { graphWindows } from '../lib/graph-preferences'

const DAY_SECONDS = 24 * 60 * 60
const WEEK_SECONDS = graphWindows[0].secs

function formatLiveTime(time: number, windowSeconds: number) {
  const date = new Date(time * 1_000)
  return date.toLocaleDateString('en-US', {
    ...(windowSeconds <= WEEK_SECONDS ? { weekday: 'short' } : { month: 'short', day: 'numeric' }),
    timeZone: 'America/Los_Angeles',
  })
}

export function PerformanceChart({
  data,
  netDeposits,
  benchmark,
  value,
  isLive,
  selectedWindow,
  onWindowChange,
}: {
  data: LivelinePoint[]
  netDeposits?: LivelinePoint[]
  benchmark?: LivelinePoint[]
  value: number
  isLive: boolean
  selectedWindow: number
  onWindowChange: (seconds: number) => void
}) {
  const { resolvedTheme } = useTheme()
  const reduceMotion = useReducedMotion()
  const firstTime = data[0]?.time ?? 0
  const lastTime = data.at(-1)?.time ?? firstTime
  const allTimeWindow = Math.max(91 * DAY_SECONDS, lastTime - firstTime + DAY_SECONDS)
  const windows = graphWindows.map((window) => ({
    ...window,
    secs: window.secs || allTimeWindow,
  }))
  const windowSeconds = selectedWindow || allTimeWindow
  const series: LivelineSeries[] = [
    {
      id: 'Value',
      data,
      value,
      color: resolvedTheme === 'dark' ? '#d9dcd8' : '#303332',
    },
    ...(netDeposits?.length
      ? [
          {
            id: 'Net deposits',
            data: netDeposits,
            value: netDeposits.at(-1)?.value ?? 0,
            color: resolvedTheme === 'dark' ? '#c78c60' : '#a46d46',
            dash: [1, 5],
          },
        ]
      : []),
    ...(benchmark?.length
      ? [
          {
            id: 'S&P 500',
            data: benchmark,
            value: benchmark.at(-1)?.value ?? 0,
            color: resolvedTheme === 'dark' ? '#76aca3' : '#477d75',
            dash: [1, 5],
          },
        ]
      : []),
  ]

  return (
    <div
      className="chart-container performance-chart live-performance-chart"
      role="img"
      aria-label="Account value over time"
    >
      <Liveline
        data={data}
        value={value}
        series={series}
        theme={resolvedTheme === 'dark' ? 'dark' : 'light'}
        color={resolvedTheme === 'dark' ? '#d9dcd8' : '#303332'}
        window={windowSeconds}
        windows={windows}
        onWindowChange={(seconds) => onWindowChange(seconds === allTimeWindow ? 0 : seconds)}
        windowStyle="text"
        seriesToggleCompact
        badgeVariant="minimal"
        tooltipY={-10}
        tooltipOutline={false}
        formatValue={formatCurrency}
        formatTime={(time) => formatLiveTime(time, windowSeconds)}
        pulse={isLive && !reduceMotion}
        momentum={!reduceMotion}
        lerpSpeed={reduceMotion ? 1 : 0.12}
        lineWidth={2.25}
        style={{ flex: 1, minHeight: 0, height: 'auto' }}
      />
    </div>
  )
}

export function AllocationChart({
  data,
  centerValue,
  centerLabel = 'Invested',
}: {
  data: Array<{ name: string; percent: number; color: string }>
  centerValue: number
  centerLabel?: string
}) {
  const chartData = data.some(({ percent }) => percent > 0)
    ? data
    : [{ name: 'No data', percent: 100, color: 'var(--surface-hover)' }]

  return (
    <div className="donut-wrap">
      <div className="donut-chart" role="img" aria-label={`${centerLabel} allocation`}>
        <PieChart responsive style={{ width: '100%', height: '100%' }}>
          <Pie
            data={chartData}
            dataKey="percent"
            nameKey="name"
            innerRadius="66%"
            outerRadius="100%"
            startAngle={90}
            endAngle={-270}
            stroke="none"
            isAnimationActive={false}
            shape={(props: PieSectorShapeProps) => (
              <Sector {...props} fill={chartData[props.index]?.color} />
            )}
          />
          <Tooltip
            cursor={false}
            isAnimationActive={false}
            content={({ active, payload }) =>
              active && payload?.[0] ? (
                <div className="allocation-tooltip">
                  <strong>{payload[0].name}</strong>
                  <span>{Number(payload[0].value).toFixed(1)}%</span>
                </div>
              ) : null
            }
          />
        </PieChart>
        <div className="donut-center">
          <small>{centerLabel}</small>
          <strong>{formatCompactCurrency(centerValue)}</strong>
        </div>
      </div>
    </div>
  )
}
