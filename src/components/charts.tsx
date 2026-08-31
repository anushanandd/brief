import {
  Area,
  AreaChart,
  Bar,
  BarChart,
  CartesianGrid,
  Cell,
  Pie,
  PieChart,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts'

import { formatCompactCurrency, formatCurrency } from '../lib/format'

type HistoryPoint = { date: string; value: number }
type AllocationPoint = { name: string; value: number; percent: number; color: string }

function ChartTooltip({
  active,
  payload,
  label,
}: {
  active?: boolean
  payload?: Array<{ value?: number; payload?: { name?: string } }>
  label?: string
}) {
  const point = payload?.[0]
  if (!active || typeof point?.value !== 'number') return null

  return (
    <div className="chart-tooltip">
      <span>{label ?? point.payload?.name}</span>
      <strong>{formatCurrency(point.value)}</strong>
    </div>
  )
}

export function NetWorthChart({ data }: { data: HistoryPoint[] }) {
  const baseline = Math.min(...data.map((point) => point.value)) * 0.96

  return (
    <div className="chart-container" role="img" aria-label="Six month net worth trend">
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
            dy={8}
          />
          <YAxis hide domain={[baseline, 'dataMax + 4000']} />
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
