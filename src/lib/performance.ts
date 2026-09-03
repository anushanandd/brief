import { formatCurrency } from './format'
import type { InvestmentActivity, Transaction } from './schema'

export type BenchmarkPoint = { date: string; value: number }

export type BrokeragePerformancePoint = {
  date: string
  value: number
  netDeposits: number
  sp500: number | null
}

export type AnnotatedPerformancePoint = BrokeragePerformancePoint & { note?: string }

export type BrokeragePerformance = {
  accountId: string
  name: string
  institution: string
  currentValue: number
  points: BrokeragePerformancePoint[]
}

const daysApart = (left: string, right: string) =>
  Math.abs(Date.parse(`${left}T00:00:00Z`) - Date.parse(`${right}T00:00:00Z`)) / 86_400_000
const closeAmount = (left: number, right: number, tolerance = 0.03) =>
  Math.abs(Math.abs(left) - Math.abs(right)) <=
  Math.max(5, Math.max(Math.abs(left), Math.abs(right)) * tolerance)
const looksLikeTransfer = (transaction: Pick<Transaction, 'category' | 'merchant'>) =>
  `${transaction.category} ${transaction.merchant}`.toLocaleLowerCase().includes('transfer')
const isCashTransfer = (activity: InvestmentActivity) =>
  activity.type === 'WITHDRAWAL' ||
  activity.type === 'CONTRIBUTION' ||
  activity.type === 'DEPOSIT' ||
  activity.type === 'TRANSFER' ||
  activity.type === 'CASH_TRANSFER' ||
  activity.type.endsWith('_TRANSFER_IN') ||
  activity.type.endsWith('_TRANSFER_OUT')

export function annotateKeyMoments(
  points: BrokeragePerformancePoint[],
  context: {
    transactions: Transaction[]
    investmentActivities: InvestmentActivity[]
    currentValue: number
    accountId: string
  },
): AnnotatedPerformancePoint[] {
  const threshold = Math.max(Math.abs(context.currentValue) * 0.005, 100)
  const moments = new Set(
    points
      .slice(1)
      .map((point, index) => ({ index: index + 1, change: point.value - points[index].value }))
      .filter(({ change }) => Math.abs(change) >= threshold)
      .toSorted((left, right) => Math.abs(right.change) - Math.abs(left.change))
      .slice(0, 5)
      .map(({ index }) => index),
  )

  return points.map((point, index) => {
    if (!moments.has(index)) return point
    const change = point.value - points[index - 1].value
    const movement = `${change > 0 ? 'Up' : 'Down'} ${formatCurrency(Math.abs(change))}`
    const transfer = context.investmentActivities
      .filter(
        (activity) =>
          activity.accountId === context.accountId &&
          isCashTransfer(activity) &&
          Math.sign(activity.amount) === Math.sign(change) &&
          daysApart(activity.date, point.date) <= 3 &&
          closeAmount(activity.amount, change, 0.35),
      )
      .toSorted(
        (left, right) =>
          daysApart(left.date, point.date) - daysApart(right.date, point.date) ||
          Math.abs(Math.abs(left.amount) - Math.abs(change)) -
            Math.abs(Math.abs(right.amount) - Math.abs(change)),
      )[0]
    if (transfer) {
      // ponytail: providers share no transfer ID, so correlate by direction, amount, and posting date.
      const investmentCounterpart = context.investmentActivities
        .filter(
          (activity) =>
            activity.accountId !== transfer.accountId &&
            isCashTransfer(activity) &&
            Math.sign(activity.amount) === -Math.sign(transfer.amount) &&
            daysApart(activity.date, transfer.date) <= 3 &&
            closeAmount(activity.amount, transfer.amount),
        )
        .toSorted(
          (left, right) =>
            daysApart(left.date, transfer.date) - daysApart(right.date, transfer.date),
        )[0]
      const counterpart = context.transactions
        .filter(
          (transaction) =>
            Math.sign(transaction.amount) === -Math.sign(transfer.amount) &&
            looksLikeTransfer(transaction) &&
            daysApart(transaction.date, transfer.date) <= 3 &&
            closeAmount(transaction.amount, transfer.amount),
        )
        .toSorted(
          (left, right) =>
            daysApart(left.date, transfer.date) - daysApart(right.date, transfer.date),
        )[0]
      const amount = formatCurrency(Math.abs(transfer.amount))
      const otherAccount = investmentCounterpart?.accountName ?? counterpart?.account ?? 'Bank'
      const from = transfer.amount < 0 ? transfer.accountName : otherAccount
      const to = transfer.amount < 0 ? otherAccount : transfer.accountName
      const sale =
        transfer.amount < 0
          ? context.investmentActivities
              .filter(
                (activity) =>
                  activity.accountId === context.accountId &&
                  activity.type === 'SELL' &&
                  activity.amount > 0 &&
                  Date.parse(activity.date) <= Date.parse(transfer.date) &&
                  daysApart(activity.date, transfer.date) <= 7 &&
                  closeAmount(activity.amount, transfer.amount, 0.25),
              )
              .toSorted(
                (left, right) =>
                  daysApart(left.date, transfer.date) - daysApart(right.date, transfer.date),
              )[0]
          : undefined
      const saleNote = sale
        ? `Sold ${sale.symbol ?? 'securities'} for ${formatCurrency(sale.amount)} · `
        : ''
      return {
        ...point,
        note: `${saleNote}${sale ? 'moved' : 'Moved'} ${amount} from ${from} to ${to}`,
      }
    }

    const transaction = context.transactions
      .filter((item) => item.date === point.date && Math.sign(item.amount) === Math.sign(change))
      .toSorted((left, right) => Math.abs(right.amount) - Math.abs(left.amount))[0]
    const counterpart = transaction
      ? context.transactions.find(
          (item) =>
            item.account !== transaction.account &&
            Math.sign(item.amount) === -Math.sign(transaction.amount) &&
            (looksLikeTransfer(item) || looksLikeTransfer(transaction)) &&
            daysApart(item.date, transaction.date) <= 3 &&
            closeAmount(item.amount, transaction.amount),
        )
      : undefined
    if (transaction && counterpart) {
      const outgoing = transaction.amount < 0 ? transaction : counterpart
      const incoming = transaction.amount > 0 ? transaction : counterpart
      return {
        ...point,
        note: `Moved ${formatCurrency(Math.abs(transaction.amount))} from ${outgoing.account} to ${incoming.account}`,
      }
    }
    return {
      ...point,
      note: transaction
        ? `${movement} · ${transaction.merchant}: ${formatCurrency(transaction.amount)}`
        : movement,
    }
  })
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
  days = 730,
): BrokeragePerformance[] {
  const dates = Array.from({ length: days }, (_, index) => utcDate(now, days - index - 1))
  const benchmarkByDate = new Map(
    benchmarkHistory.map((point) => [point.date.slice(0, 10), point.value]),
  )

  const accounts = snaptrade.accounts.flatMap((account) => {
    const id = String(account.id ?? '')
    if (!id) return []
    const accountId = String(account.accountId ?? `snaptrade:${id}`)

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
        accountId,
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
