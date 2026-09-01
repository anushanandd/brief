import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Line,
  LineChart,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { isTauri, openExternalUrl } from '../lib/api'
import { formatCompactCurrency, formatCurrency } from '../lib/format'

type HistoryPoint = { date: string; value: number }
type AllocationPoint = { name: string; value: number; percent: number; color: string }
type PerformancePoint = {
  date: string
  value: number
  netDeposits: number
  sp500: number | null
}
type HoldingPoint = { ticker: string; value: number; color: string }

function formatHistoryDate(value: string, long = false) {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return value
  return new Date(`${value}T00:00:00Z`).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    ...(long ? { year: 'numeric' } : {}),
    timeZone: 'UTC',
  })
}

function ChartTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean
  payload?: Array<{ value?: number; payload?: { name?: string; ticker?: string } }>
  label?: string
}) {
  const point = payload?.[0]
  if (!active || typeof point?.value !== 'number') return null

  return (
    <div className="chart-tooltip">
      <span>
        {label ? formatHistoryDate(label, true) : (point.payload?.name ?? point.payload?.ticker)}
      </span>
      <strong>{formatCurrency(point.value)}</strong>
    </div>
  )
}

export function NetWorthChart({ data }: { data: HistoryPoint[] }) {
  const values = data.map((point) => point.value)
  const minimum = Math.min(...values)
  const maximum = Math.max(...values)
  const padding = Math.max((maximum - minimum) * 0.12, Math.max(Math.abs(maximum), 1) * 0.025)

  return (
    <div className="chart-container" role="img" aria-label="Net worth trend">
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 12, right: 4, left: 0, bottom: 0 }}>
          <defs>
            <linearGradient id="netWorthFill" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="var(--chart-primary)" stopOpacity={0.22} />
              <stop offset="100%" stopColor="var(--chart-primary)" stopOpacity={0} />
            </linearGradient>
          </defs>
          <CartesianGrid vertical={false} stroke="var(--border-subtle)" />
          <XAxis
            dataKey="date"
            axisLine={false}
            tickLine={false}
            tick={{ fill: 'var(--text-tertiary)', fontSize: 11 }}
            tickFormatter={(value: string) => formatHistoryDate(value)}
            dy={8}
          />
          <YAxis hide domain={[minimum - padding, maximum + padding]} />
          <Tooltip content={<ChartTooltip />} cursor={{ stroke: 'var(--border-strong)' }} />
          <Area
            type="monotone"
            dataKey="value"
            stroke="var(--chart-primary)"
            strokeWidth={2.25}
            fill="url(#netWorthFill)"
            activeDot={{ r: 4, fill: 'var(--chart-primary)', strokeWidth: 0 }}
            animationDuration={550}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  )
}

function PerformanceTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean
  payload?: Array<{ color?: string; name?: string; value?: number }>
  label?: string
}) {
  const values = payload?.filter((item) => typeof item.value === 'number') ?? []
  if (!active || !values.length) return null

  return (
    <div className="chart-tooltip performance-tooltip">
      <span>{label ? formatHistoryDate(label, true) : ''}</span>
      {values.map((item) => (
        <div key={item.name}>
          <i style={{ background: item.color }} />
          <span>{item.name}</span>
          <strong>{formatCurrency(item.value ?? 0)}</strong>
        </div>
      ))}
    </div>
  )
}

export function PerformanceChart({
  data,
  valueLabel = 'Value',
  showNetDeposits = true,
}: {
  data: PerformancePoint[]
  valueLabel?: string
  showNetDeposits?: boolean
}) {
  if (!data.length) {
    return <p className="holdings-pie-empty">Refresh to load performance history.</p>
  }

  const values = data.flatMap((point) =>
    [point.value, ...(showNetDeposits ? [point.netDeposits] : []), point.sp500].filter(
      (value): value is number => typeof value === 'number',
    ),
  )
  const minimum = Math.min(...values)
  const maximum = Math.max(...values)
  const padding = Math.max((maximum - minimum) * 0.12, Math.max(Math.abs(maximum), 1) * 0.025)

  return (
    <div className="performance-chart-wrap">
      <div className="performance-legend" aria-label="Chart series">
        <span>
          <i className="value" />
          {valueLabel}
        </span>
        {showNetDeposits ? (
          <span>
            <i className="deposits" />
            Net deposits
          </span>
        ) : null}
        <span>
          <i className="benchmark" />
          S&amp;P 500
        </span>
      </div>
      <div
        className="chart-container performance-chart"
        role="img"
        aria-label="Brokerage performance"
      >
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 12, right: 5, left: 5, bottom: 8 }}>
            <CartesianGrid vertical={false} stroke="var(--border-subtle)" />
            <XAxis dataKey="date" hide />
            <YAxis hide domain={[minimum - padding, maximum + padding]} />
            <Tooltip content={<PerformanceTooltip />} cursor={{ stroke: 'var(--border-strong)' }} />
            <Line
              type="monotone"
              dataKey="value"
              name={valueLabel}
              stroke="var(--chart-primary)"
              strokeWidth={2.4}
              dot={false}
              activeDot={{ r: 4, strokeWidth: 0 }}
              isAnimationActive={false}
            />
            {showNetDeposits ? (
              <Line
                type="monotone"
                dataKey="netDeposits"
                name="Net deposits"
                stroke="var(--chart-secondary)"
                strokeWidth={1.8}
                strokeDasharray="3 4"
                dot={false}
                activeDot={{ r: 3, strokeWidth: 0 }}
                isAnimationActive={false}
              />
            ) : null}
            <Line
              type="monotone"
              dataKey="sp500"
              name="S&amp;P 500"
              stroke="var(--chart-benchmark)"
              strokeWidth={2.25}
              strokeDasharray="6 4"
              dot={false}
              activeDot={{ r: 3, strokeWidth: 0 }}
              connectNulls
              isAnimationActive={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
    </div>
  )
}

export function AllocationChart({
  data,
  centerValue,
}: {
  data: AllocationPoint[]
  centerValue: number
}) {
  return (
    <div className="donut-wrap">
      <div className="donut-chart" role="img" aria-label="Portfolio allocation">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={data}
              dataKey="value"
              nameKey="name"
              cx="50%"
              cy="50%"
              innerRadius="67%"
              outerRadius="88%"
              paddingAngle={1.5}
              stroke="none"
              animationDuration={500}
            >
              {data.map((entry) => (
                <Cell key={entry.name} fill={entry.color} />
              ))}
            </Pie>
            <Tooltip content={<ChartTooltip />} />
          </PieChart>
        </ResponsiveContainer>
        <div className="donut-center">
          <small>Invested</small>
          <strong>{formatCompactCurrency(centerValue)}</strong>
        </div>
      </div>
      <ul className="chart-legend">
        {data.map((item) => (
          <li key={item.name}>
            <span className="legend-swatch" style={{ backgroundColor: item.color }} />
            <span>{item.name}</span>
            <strong>{item.percent.toFixed(1)}%</strong>
          </li>
        ))}
      </ul>
    </div>
  )
}

export function HoldingsPieChart({ data }: { data: HoldingPoint[] }) {
  const holdings = [
    ...data
      .reduce((grouped, holding) => {
        const existing = grouped.get(holding.ticker)
        grouped.set(holding.ticker, {
          ...holding,
          value: (existing?.value ?? 0) + holding.value,
          color: existing?.color ?? holding.color,
        })
        return grouped
      }, new Map<string, HoldingPoint>())
      .values(),
  ].toSorted((left, right) => right.value - left.value)
  const total = holdings.reduce((sum, holding) => sum + holding.value, 0)

  if (!holdings.length) {
    return <p className="holdings-pie-empty">Refresh to load your brokerage holdings.</p>
  }

  return (
    <div className="holdings-pie-wrap">
      <div className="donut-chart holdings-donut" role="img" aria-label="Holdings allocation">
        <ResponsiveContainer width="100%" height="100%">
          <PieChart>
            <Pie
              data={holdings}
              dataKey="value"
              nameKey="ticker"
              cx="50%"
              cy="50%"
              innerRadius="64%"
              outerRadius="88%"
              paddingAngle={1.5}
              stroke="none"
              animationDuration={500}
            >
              {holdings.map((holding) => (
                <Cell key={holding.ticker} fill={holding.color} />
              ))}
            </Pie>
            <Tooltip content={<ChartTooltip />} />
          </PieChart>
        </ResponsiveContainer>
        <div className="donut-center">
          <small>Holdings</small>
          <strong>{formatCompactCurrency(total)}</strong>
        </div>
      </div>
      <ul className="holdings-pie-legend">
        {holdings.map((holding) => {
          const yahooUrl = `https://finance.yahoo.com/quote/${encodeURIComponent(holding.ticker)}`
          return (
            <li key={holding.ticker}>
              <span className="legend-swatch" style={{ backgroundColor: holding.color }} />
              <a
                href={yahooUrl}
                target="_blank"
                rel="noopener noreferrer"
                onClick={(event) => {
                  if (!isTauri()) return
                  event.preventDefault()
                  void openExternalUrl(yahooUrl)
                }}
              >
                {holding.ticker}
              </a>
              <strong>{total ? ((holding.value / total) * 100).toFixed(1) : '0.0'}%</strong>
            </li>
          )
        })}
      </ul>
    </div>
  )
}

export function DividendChart({ data }: { data: Array<{ month: string; value: number }> }) {
  return (
    <div className="chart-container compact-chart" role="img" aria-label="Monthly dividends">
      <ResponsiveContainer width="100%" height="100%">
        <BarChart
          data={data}
          barCategoryGap="42%"
          margin={{ top: 8, right: 0, left: 0, bottom: 0 }}
        >
          <CartesianGrid vertical={false} stroke="var(--border-subtle)" />
          <XAxis
            dataKey="month"
            axisLine={false}
            tickLine={false}
            tick={{ fill: 'var(--text-tertiary)', fontSize: 11 }}
            dy={8}
          />
          <YAxis hide />
          <Tooltip content={<ChartTooltip />} cursor={{ fill: 'var(--surface-hover)' }} />
          <Bar
            dataKey="value"
            fill="var(--chart-secondary)"
            radius={[4, 4, 1, 1]}
            animationDuration={450}
          />
        </BarChart>
      </ResponsiveContainer>
    </div>
  )
}
