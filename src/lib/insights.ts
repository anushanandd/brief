import { formatCurrency, formatPercent } from './format'
import { transactionMarkKind } from './logos'
import type { FinanceSnapshot, MarketNewsArticle, SnapshotChange } from './schema'
import { isSpendingTransaction, transactionDateKey } from './spending'

export const holdingImpact = (value: number | null, changePct: number | null) => {
  if (value == null || changePct == null) return 0
  const ratio = 1 + changePct / 100
  return ratio > 0 ? value - value / ratio : value
}

export type WeeklyBriefingCandidate = {
  key: string
  netWorth?: {
    change: number
    changePct: number
    date: string
    accountChanges: SnapshotChange['accountChanges']
  }
  stock?: {
    ticker: string
    name: string
    changePct: number
    impact: number
  }
  transactions: FinanceSnapshot['transactions']
}

export function weeklyBriefingCandidate(
  data: FinanceSnapshot | undefined,
): WeeklyBriefingCandidate | undefined {
  if (!data) return undefined
  const recordedChange = data.lastChange
  const evidenceDay = new Date(`${data.updatedAt.slice(0, 10)}T00:00:00Z`)
  evidenceDay.setUTCDate(evidenceDay.getUTCDate() - ((evidenceDay.getUTCDay() + 6) % 7))
  const week = evidenceDay.toISOString().slice(0, 10)
  const change = recordedChange?.netWorthChange ?? 0
  const previousNetWorth = recordedChange?.previousNetWorth ?? data.netWorth - change
  const changePct = previousNetWorth ? (change / Math.abs(previousNetWorth)) * 100 : 0
  const netWorth =
    recordedChange &&
    recordedChange.comparisonComplete !== false &&
    recordedChange.observedAt === data.updatedAt &&
    Math.abs(change) >= Math.max(500, Math.abs(previousNetWorth) * 0.0025)
      ? {
          change,
          changePct,
          date: recordedChange.observedAt.slice(0, 10),
          accountChanges: recordedChange.accountChanges,
        }
      : undefined

  const minimumImpact = Math.max(100, Math.abs(data.netWorth) * 0.001)
  const stock = data.holdings
    .map((holding) => ({
      ticker: holding.ticker,
      name: holding.name,
      changePct: holding.weeklyChangePct ?? 0,
      impact: holdingImpact(holding.value, holding.weeklyChangePct),
    }))
    .filter(
      (holding) =>
        Math.abs(holding.changePct) >= 8 ||
        (Math.abs(holding.changePct) >= 4 && Math.abs(holding.impact) >= minimumImpact),
    )
    .toSorted(
      (left, right) =>
        Math.abs(right.changePct) - Math.abs(left.changePct) ||
        Math.abs(right.impact) - Math.abs(left.impact),
    )[0]

  const transactionThreshold = Math.max(500, Math.abs(data.spending.monthTotal) * 0.1)
  const transactions = data.transactions
    .filter((transaction) => {
      const date = transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)
      const amount = Math.abs(transaction.amount)
      return (
        date >= week &&
        date <= data.updatedAt.slice(0, 10) &&
        ((transactionMarkKind(transaction) === 'payment' && amount >= 250) ||
          amount >= transactionThreshold)
      )
    })
    .toSorted(
      (left, right) =>
        Number(transactionMarkKind(right) === 'payment') -
          Number(transactionMarkKind(left) === 'payment') ||
        Math.abs(right.amount) - Math.abs(left.amount),
    )
    .slice(0, 5)

  if (!netWorth && !stock && !transactions.length) return undefined
  return {
    key: [
      week,
      netWorth?.date ?? '',
      stock?.ticker ?? '',
      stock ? Math.trunc(stock.changePct) : '',
      ...transactions.map(({ id }) => id),
    ].join(':'),
    netWorth,
    stock,
    transactions,
  }
}

export function briefingEvidence(
  candidate: WeeklyBriefingCandidate,
  news?: MarketNewsArticle[],
): string {
  const lines = [
    'Add context without repeating any numeric values from the verified changes below.',
  ]
  if (candidate.netWorth) {
    lines.push(
      `Verified net worth direction: ${candidate.netWorth.change >= 0 ? 'increased' : 'decreased'}.`,
    )
    if (candidate.netWorth.accountChanges.length) {
      lines.push('Verified account balance directions:')
      for (const account of candidate.netWorth.accountChanges.slice(0, 5)) {
        lines.push(`- ${account.name}: ${account.change >= 0 ? 'increased' : 'decreased'}.`)
      }
    }
    if (candidate.transactions.length) {
      lines.push('Notable weekly transaction evidence; these are possible contributors:')
      for (const transaction of candidate.transactions) {
        lines.push(
          `- ${transaction.merchant}; ${transaction.category}; ${transaction.pending ? 'pending' : 'posted'}.`,
        )
      }
    } else {
      lines.push('No notable weekly transaction evidence is available.')
    }
  }
  if (candidate.stock) {
    lines.push(
      `Verified material holding move: ${candidate.stock.ticker} (${candidate.stock.name}) moved ${candidate.stock.changePct >= 0 ? 'higher' : 'lower'} and had a material portfolio effect.`,
    )
    if (news?.length) {
      lines.push('Recent news evidence:')
      for (const article of news) {
        lines.push(`- ${article.headline}. ${article.summary}`.trim())
      }
    } else {
      lines.push(
        news
          ? 'No recent company news was found; do not invent a catalyst.'
          : 'Recent company news is unavailable; do not invent a catalyst.',
      )
    }
  }
  return lines.join('\n')
}

export type HomeInsightSection = {
  title: 'Key changes' | 'Daily summary' | 'Weekly summary'
  lines: string[]
}

const limitSentences = (lines: string[]) =>
  [...new Intl.Segmenter('en', { granularity: 'sentence' }).segment(lines.join(' '))]
    .slice(0, 2)
    .map(({ segment }) => segment.trim())

const generatedContext = (value?: string) => {
  const context = value
    ?.replace(/^\s*(?:[-*]\s*)?(?:title:|what changed:)\s*/i, '')
    .replace(/\s+/g, ' ')
    .trim()
  return context &&
    context !== 'NO_CONTEXT' &&
    !/[\d$€£¥%]/.test(context) &&
    !/\b(?:can't|cannot|unable|not enough|without repeating)\b/i.test(context)
    ? context
    : undefined
}

const periodSummary = (
  data: FinanceSnapshot,
  start: string,
  end: string,
  label: 'Today' | 'The last seven days',
) => {
  const transactions = data.transactions.filter((transaction) => {
    const date = transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt)
    return !transaction.pending && date >= start && date <= end
  })
  if (!transactions.length) return [`${label} had no imported posted transactions.`]

  const lines = [
    `${label} included ${transactions.length} imported posted transaction${transactions.length === 1 ? '' : 's'}.`,
  ]
  const spending = transactions
    .filter(isSpendingTransaction)
    .reduce((total, transaction) => total + Math.abs(transaction.amount), 0)
  const incomeTransactions = transactions.filter(
    (transaction) =>
      transaction.amount > 0 &&
      ['income', 'interest', 'dividend'].includes(transactionMarkKind(transaction)),
  )
  const income = incomeTransactions.reduce((total, transaction) => total + transaction.amount, 0)
  const totals = [
    spending ? `Spending was ${formatCurrency(spending)}` : '',
    income ? `income was ${formatCurrency(income)}` : '',
  ].filter(Boolean)
  if (totals.length) {
    const summary = totals.join(' and ')
    lines.push(`${summary[0]?.toUpperCase()}${summary.slice(1)}.`)
  }
  if (incomeTransactions.length) {
    const sources = [...new Set(incomeTransactions.map(({ merchant }) => merchant))].slice(0, 3)
    lines.push(`Imported income sources included ${sources.join(', ')}.`)
  }
  return lines
}

export function homeInsightSections(
  data: FinanceSnapshot,
  candidate?: WeeklyBriefingCandidate,
  modelContext?: string,
): HomeInsightSection[] {
  const end = data.updatedAt.slice(0, 10)
  const startDate = new Date(`${end}T00:00:00Z`)
  startDate.setUTCDate(startDate.getUTCDate() - 6)
  const start = startDate.toISOString().slice(0, 10)
  const dailyMover = data.holdings
    .filter(({ dailyChangePct }) => dailyChangePct != null)
    .toSorted(
      (left, right) => Math.abs(right.dailyChangePct ?? 0) - Math.abs(left.dailyChangePct ?? 0),
    )[0]
  const weeklyMover = data.holdings
    .filter(({ weeklyChangePct }) => weeklyChangePct != null)
    .toSorted(
      (left, right) => Math.abs(right.weeklyChangePct ?? 0) - Math.abs(left.weeklyChangePct ?? 0),
    )[0]
  const context = generatedContext(modelContext)

  return [
    {
      title: 'Key changes',
      lines: limitSentences(
        candidate
          ? fallbackWeeklyBriefing(candidate).split('\n\n')
          : ['No significant balance, transaction, or holding movements crossed your thresholds.'],
      ),
    },
    {
      title: 'Daily summary',
      lines: limitSentences([
        ...periodSummary(data, end, end, 'Today'),
        ...(dailyMover
          ? [
              `${dailyMover.ticker} had the largest daily holding move at ${formatPercent(dailyMover.dailyChangePct)}.`,
            ]
          : []),
      ]),
    },
    {
      title: 'Weekly summary',
      lines: limitSentences([
        ...periodSummary(data, start, end, 'The last seven days'),
        ...(weeklyMover
          ? [
              `${weeklyMover.ticker} had the largest seven-day holding move at ${formatPercent(weeklyMover.weeklyChangePct)}.`,
            ]
          : []),
        ...(context ? [context] : []),
      ]),
    },
  ]
}

export function fallbackWeeklyBriefing(candidate: WeeklyBriefingCandidate): string {
  const parts: string[] = []
  if (candidate.netWorth) {
    parts.push(
      `This week's latest net worth change was ${formatCurrency(candidate.netWorth.change)}.`,
    )
  }
  if (candidate.transactions.length) {
    const transactions = candidate.transactions
      .slice(0, 3)
      .map((transaction) => `${transaction.merchant} at ${formatCurrency(transaction.amount)}`)
      .join('; ')
    parts.push(`Notable weekly activity included ${transactions}.`)
  }
  if (candidate.stock) {
    parts.push(
      `This week's holding highlight was ${candidate.stock.ticker}, which moved ${formatPercent(candidate.stock.changePct)} and changed the portfolio by approximately ${formatCurrency(candidate.stock.impact)}.`,
    )
  }
  return parts.join('\n\n')
}
