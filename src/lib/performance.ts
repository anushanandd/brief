export type BenchmarkPoint = { date: string; value: number }

export type BrokeragePerformancePoint = {
  date: string
  value: number
  netDeposits: number
  sp500: number | null
}

export type BrokeragePerformance = {
  accountId: string
  name: string
  institution: string
  currentValue: number
  points: BrokeragePerformancePoint[]
}

type RawSnapTradeData = {
  accounts: Array<Record<string, any>>
  activities: Record<string, Array<Record<string, any>>>
  balanceHistory?: Record<string, Array<Record<string, any>>>
}

const round = (value: number) => Math.round(value * 100) / 100
const number = (value: unknown) => {
  const parsed = typeof value === 'number' ? value : Number(value ?? 0)
  return Number.isFinite(parsed) ? parsed : 0
}

function utcDate(date: Date, offset: number) {
  const value = new Date(Date.UTC(date.getUTCFullYear(), date.getUTCMonth(), date.getUTCDate()))
  value.setUTCDate(value.getUTCDate() - offset)
  return value.toISOString().slice(0, 10)
}

function activityFlow(activity: Record<string, any>) {
  const type = String(activity.type ?? '')
    .trim()
    .toLocaleUpperCase()
    .replaceAll(/[^A-Z0-9]+/g, '_')
  const amount = number(activity.amount)

  if (type.includes('CONTRIBUTION') || type === 'DEPOSIT' || type.endsWith('_TRANSFER_IN')) {
    return Math.abs(amount)
  }
  if (type.includes('WITHDRAWAL') || type.endsWith('_TRANSFER_OUT')) {
    return -Math.abs(amount)
  }
  if (type === 'TRANSFER' || type === 'CASH_TRANSFER') return amount
  return 0
}

function valueHistory(
  rawHistory: Array<Record<string, any>>,
  flows: Map<string, number>,
  currentValue: number,
  dates: string[],
) {
  const reported = new Map<string, number>(
    rawHistory
      .flatMap((point): Array<[string, number]> => {
        const date = String(point.date ?? '').slice(0, 10)
        const rawValue = point.total_value ?? point.value
        const value = typeof rawValue === 'number' ? rawValue : Number(rawValue)
        return /^\d{4}-\d{2}-\d{2}$/.test(date) && Number.isFinite(value) ? [[date, value]] : []
      })
      .toSorted(([left], [right]) => left.localeCompare(right)),
  )
  reported.set(dates.at(-1) ?? '', currentValue)

  if (reported.size > 1) {
    const ordered = [...reported].toSorted(([left], [right]) => left.localeCompare(right))
    let cursor = 0
    let lastValue = ordered[0]?.[1] ?? currentValue
    return dates.map((date) => {
      while (cursor < ordered.length && (ordered[cursor]?.[0] ?? '') <= date) {
        lastValue = ordered[cursor]?.[1] ?? lastValue
        cursor += 1
      }
      return round(lastValue)
    })
  }

  let value = currentValue
  const reconstructed = dates.toReversed().map((date) => {
    const point = round(value)
    value = round(value - (flows.get(date) ?? 0))
    return point
  })
  return reconstructed.toReversed()
}

export function buildBrokeragePerformance(
  snaptrade: RawSnapTradeData,
  benchmarkHistory: BenchmarkPoint[],
  now: Date,
  days = 90,
): BrokeragePerformance[] {
  const dates = Array.from({ length: days }, (_, index) => utcDate(now, days - index - 1))
  const benchmarkByDate = new Map(
    benchmarkHistory.map((point) => [point.date.slice(0, 10), point.value]),
  )

  const accounts = snaptrade.accounts.flatMap((account) => {
    const id = String(account.id ?? '')
    if (!id) return []

    const currentValue = round(number(account.balance?.total?.amount))
    const flows = new Map<string, number>()
    for (const activity of snaptrade.activities[id] ?? []) {
      const date = String(activity.trade_date ?? activity.settlement_date ?? '').slice(0, 10)
      const flow = activityFlow(activity)
      if (/^\d{4}-\d{2}-\d{2}$/.test(date) && flow) {
        flows.set(date, round((flows.get(date) ?? 0) + flow))
      }
    }

    const values = valueHistory(snaptrade.balanceHistory?.[id] ?? [], flows, currentValue, dates)
    let netDeposits = values[0] ?? currentValue
    let benchmarkValue = netDeposits
    let lastIndex: number | undefined
    const points = dates.map((date, index) => {
      const benchmarkIndex = benchmarkByDate.get(date) ?? lastIndex
      if (index > 0) {
        if (benchmarkIndex && lastIndex) benchmarkValue *= benchmarkIndex / lastIndex
        const flow = flows.get(date) ?? 0
        netDeposits += flow
        benchmarkValue += flow
      }
      lastIndex = benchmarkIndex
      return {
        date,
        value: values[index] ?? currentValue,
        netDeposits: round(netDeposits),
        sp500: benchmarkIndex ? round(benchmarkValue) : null,
      }
    })

    return [
      {
        accountId: `snaptrade:${id}`,
        name: String(account.name ?? account.raw_type ?? 'Brokerage account'),
        institution: String(account.institution_name ?? 'Brokerage'),
        currentValue,
        points,
      },
    ]
  })

  if (!accounts.length) return accounts
  const totalPoints = dates.map((date, index) => {
    const accountPoints = accounts.map((account) => account.points[index])
    const benchmarkValues = accountPoints.flatMap((point) =>
      typeof point?.sp500 === 'number' ? [point.sp500] : [],
    )
    return {
      date,
      value: round(accountPoints.reduce((sum, point) => sum + (point?.value ?? 0), 0)),
      netDeposits: round(accountPoints.reduce((sum, point) => sum + (point?.netDeposits ?? 0), 0)),
      sp500: benchmarkValues.length
        ? round(benchmarkValues.reduce((sum, value) => sum + value, 0))
        : null,
    }
  })

  return [
    {
      accountId: 'total',
      name: 'Total',
      institution: 'All brokerages',
      currentValue: round(accounts.reduce((sum, account) => sum + account.currentValue, 0)),
      points: totalPoints,
    },
    ...accounts,
  ]
}
