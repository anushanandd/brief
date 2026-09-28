import { formatActivityName, formatCurrency, formatPercent } from './format'
import { holdingDetail } from './holding-detail'
import { expectedMoneyEvents, recurringMoneyKey } from './money'
import type { FinanceSnapshot } from './schema'
import { buildPlatinumBenefitTracker, transactionDateKey } from './spending'

const dayMs = 86400000

type CandidateKind =
  | 'transaction'
  | 'trade'
  | 'account-movement'
  | 'holding'
  | 'account'
  | 'upcoming'
  | 'benefit'
  | 'warning'
  | 'gap'

type Candidate = {
  id: string
  kind: CandidateKind
  score: number
  sentence: string
  date?: string
  amount?: number
  recurringId?: string
  direction?: 'inflow' | 'outflow'
  formattedAmount?: string
}

export type HomeSummaryResult = {
  sentences: [string, string]
  evidence: string
  context: Pick<Candidate, 'id' | 'kind' | 'score'>
  forward: Pick<Candidate, 'id' | 'kind' | 'score'>
}

const clamp = (value: number, minimum = 0, maximum = 1) =>
  Math.min(maximum, Math.max(minimum, value))

const percentilePoints = (value: number, values: number[], maximum: number) => {
  const sorted = values.toSorted((left, right) => left - right)
  if (sorted.length < 2) return maximum
  return Math.round(
    maximum * (sorted.findLastIndex((candidate) => candidate <= value) / (sorted.length - 1)),
  )
}

const recencyPoints = (date: string, savedDate: string, rangeSeconds: number) => {
  const age = Math.max(0, (Date.parse(savedDate) - Date.parse(date)) / dayMs)
  const window = clamp(rangeSeconds ? rangeSeconds / 86400 : 30, 7, 30)
  return Math.round(20 * clamp(1 - age / window))
}

const urgencyPoints = (days: number) =>
  days <= 3 ? 30 : days <= 7 ? 25 : days <= 14 ? 18 : days <= 30 ? 10 : 0

const signedCurrency = (value: number) => `${value > 0 ? '+' : ''}${formatCurrency(value)}`

const contextOrder: CandidateKind[] = [
  'account-movement',
  'transaction',
  'trade',
  'holding',
  'warning',
  'account',
  'gap',
]

const rankCandidates = (candidates: Candidate[], future = false) =>
  candidates.toSorted(
    (left, right) =>
      right.score - left.score ||
      (future
        ? (left.date ?? '9999').localeCompare(right.date ?? '9999')
        : (right.date ?? '').localeCompare(left.date ?? '')) ||
      Math.abs(right.amount ?? 0) - Math.abs(left.amount ?? 0) ||
      contextOrder.indexOf(left.kind) - contextOrder.indexOf(right.kind) ||
      left.id.localeCompare(right.id),
  )

export function homeSummaryTiming(date: string, today: string) {
  const days = Math.round((Date.parse(date) - Date.parse(today)) / dayMs)
  const [, month, day] = date.split('-')
  const relative =
    days === 0
      ? 'today'
      : days === 1
        ? 'in 1 day'
        : days > 1
          ? `in ${days} days`
          : `${Math.abs(days)} days ago`
  return `${relative} (${Number(month)}/${Number(day)})`
}

export function buildHomeSummary(
  data: FinanceSnapshot,
  rangeSeconds: number,
  spendingAccountId?: string,
  today = data.updatedAt.slice(0, 10),
): HomeSummaryResult {
  const savedDate = data.updatedAt.slice(0, 10)
  const start = rangeSeconds
    ? new Date(Date.parse(data.updatedAt) - rangeSeconds * 1000).toISOString().slice(0, 10)
    : (data.netWorthHistory[0]?.date ?? savedDate)
  const accountById = new Map(data.accounts.map((account) => [account.id, account]))
  const supportedAccount = (accountId?: string) => {
    const currency = accountId ? accountById.get(accountId)?.currency : undefined
    return !currency || currency === 'USD'
  }

  const upcomingEvents = expectedMoneyEvents(data, today).filter(
    ({ date, state }) =>
      state === 'expected' && date >= today && Date.parse(date) <= Date.parse(today) + 30 * dayMs,
  )
  const upcomingAmounts = upcomingEvents.map(({ amount }) => Math.abs(amount))
  const forwardCandidates: Candidate[] = upcomingEvents.map((event) => {
    const days = Math.round((Date.parse(event.date) - Date.parse(today)) / dayMs)
    const amount = formatCurrency(Math.abs(event.amount))
    return {
      id: event.id,
      kind: 'upcoming',
      score:
        40 +
        urgencyPoints(days) +
        percentilePoints(Math.abs(event.amount), upcomingAmounts, 15) +
        15,
      date: event.date,
      amount: Math.abs(event.amount),
      recurringId: event.recurringId,
      direction: event.amount < 0 ? 'outflow' : 'inflow',
      formattedAmount: amount,
      sentence: `Estimated ${event.kind === 'credit' ? 'credit' : event.amount < 0 ? 'payment' : 'income'} of ${amount} for ${formatActivityName(event.title).slice(0, 80)} ${homeSummaryTiming(event.date, today)}.`,
    }
  })

  if (spendingAccountId) {
    const benefits = buildPlatinumBenefitTracker(
      data.transactions.filter(({ accountId }) => accountId === spendingAccountId),
      `${today}T12:00:00`,
      data.updatedAt,
    ).filter(({ cadence }) => cadence !== 'renewal' && cadence !== 'purchase')
    for (const benefit of benefits) {
      const uncertainty = benefit.periodUncertain || benefit.snapshotBeforeWindow
      const state = uncertainty
        ? 15
        : benefit.status === 'matched'
          ? 15
          : benefit.remainingAmount != null && benefit.remainingAmount > 0
            ? 10
            : 0
      const materiality =
        benefit.remainingAmount != null && benefit.cap > 0
          ? Math.round(15 * clamp(benefit.remainingAmount / benefit.cap))
          : 0
      const sentence = benefit.snapshotBeforeWindow
        ? `${benefit.name} cannot be estimated because saved data predates the period ending ${benefit.reset}.`
        : benefit.periodUncertain
          ? `${benefit.name} has uncertain period attribution before ${benefit.reset}; detected credit is ${formatCurrency(benefit.creditedAmount)}.`
          : benefit.status === 'matched'
            ? `${benefit.name} has a matched purchase awaiting a detected credit before ${benefit.reset}.`
            : benefit.remainingAmount != null && benefit.remainingAmount > 0
              ? `${benefit.name} has an estimated ${formatCurrency(benefit.remainingAmount)} remaining before ${benefit.reset}.`
              : `${benefit.name} has a saved deadline of ${benefit.reset}; detected credit is ${formatCurrency(benefit.creditedAmount)}.`
      forwardCandidates.push({
        id: benefit.id,
        kind: 'benefit',
        score: 25 + urgencyPoints(benefit.daysRemaining) + materiality + state,
        date: benefit.windowEnd,
        amount: benefit.remainingAmount ?? benefit.creditedAmount,
        sentence,
      })
    }
  }

  const selectedForward = rankCandidates(forwardCandidates, true).find(
    ({ score }) => score >= 40,
  ) ?? {
    id: 'forward-gap',
    kind: 'gap' as const,
    score: 0,
    sentence: 'No upcoming estimate or benefit deadline was found in saved data.',
  }

  const contextCandidates: Candidate[] = []
  const transactions = data.transactions
    .map((transaction) => ({
      transaction,
      date: transactionDateKey(transaction.postedOn ?? transaction.date, data.updatedAt),
    }))
    .filter(
      ({ transaction, date }) =>
        !transaction.pending &&
        date >= start &&
        date <= savedDate &&
        supportedAccount(transaction.accountId),
    )
  const canonicalAccount = (accountId?: string) =>
    accountId ? (data.accountLinks[accountId] ?? accountId) : undefined
  // ponytail: exact one-to-one pairing avoids hiding coincidental activity; add grouped
  // reconciliation only if split or fee-adjusted internal transfers become common.
  const pairedTransferIds = new Set<string>()
  const transfers = transactions
    .filter(({ transaction }) => transaction.classification.kind === 'transfer')
    .toSorted(({ transaction: left }, { transaction: right }) => left.id.localeCompare(right.id))
  for (const outgoing of transfers.filter(({ transaction }) => transaction.amount < 0)) {
    const incoming = transfers.find(
      ({ transaction, date }) =>
        transaction.amount > 0 &&
        canonicalAccount(transaction.accountId) !==
          canonicalAccount(outgoing.transaction.accountId) &&
        !pairedTransferIds.has(transaction.id) &&
        Math.round(transaction.amount * 100) + Math.round(outgoing.transaction.amount * 100) ===
          0 &&
        Math.abs(Date.parse(date) - Date.parse(outgoing.date)) <= 3 * dayMs,
    )
    if (!incoming) continue
    pairedTransferIds.add(outgoing.transaction.id)
    pairedTransferIds.add(incoming.transaction.id)
  }
  const transactionAmounts = transactions
    .filter(({ transaction }) => !pairedTransferIds.has(transaction.id))
    .map(({ transaction }) => Math.abs(transaction.amount))
  const newTransactions = new Set(data.lastChange?.newTransactionIds ?? [])
  for (const { transaction, date } of transactions) {
    if (pairedTransferIds.has(transaction.id)) continue
    const kind = transaction.classification.kind
    const base = ['fee', 'tax', 'reimbursement'].includes(kind)
      ? 30
      : ['income', 'dividend', 'interest'].includes(kind)
        ? 28
        : kind === 'transfer'
          ? 25
          : 25
    const recurringId = recurringMoneyKey(transaction)
    const transferPenalty = kind === 'transfer' ? 20 : 0
    const overlapPenalty = recurringId === selectedForward.recurringId ? 25 : 0
    contextCandidates.push({
      id: transaction.id,
      kind: 'transaction',
      score:
        base +
        recencyPoints(date, savedDate, rangeSeconds) +
        percentilePoints(Math.abs(transaction.amount), transactionAmounts, 20) +
        (newTransactions.has(transaction.id) ? 15 : 0) +
        10 -
        transferPenalty -
        overlapPenalty,
      date,
      amount: transaction.amount,
      recurringId,
      sentence:
        kind === 'transfer'
          ? `Recent saved activity includes a transfer of ${formatCurrency(Math.abs(transaction.amount))} for ${formatActivityName(transaction.merchant).slice(0, 80)} on ${date}.`
          : `Recent saved activity includes a ${transaction.amount < 0 ? 'outflow' : 'inflow'} of ${formatCurrency(Math.abs(transaction.amount))} for ${formatActivityName(transaction.merchant).slice(0, 80)} on ${date}.`,
    })
  }

  const trades = data.trades
    .map((trade) => ({
      trade,
      date: transactionDateKey(trade.date, data.updatedAt),
    }))
    .filter(
      ({ trade, date }) => date >= start && date <= savedDate && supportedAccount(trade.accountId),
    )
  const tradeAmounts = trades.map(({ trade }) => Math.abs(trade.amount))
  for (const { trade, date } of trades) {
    const saleWithGain =
      /sell|sale/i.test(trade.type) &&
      trade.realizedGainMethod === 'estimated-fifo' &&
      trade.estimatedRealizedGainPct != null
    contextCandidates.push({
      id: trade.id,
      kind: 'trade',
      score:
        30 +
        recencyPoints(date, savedDate, rangeSeconds) +
        percentilePoints(Math.abs(trade.amount), tradeAmounts, 20) +
        (trade.ticker ? 10 : 5) +
        (saleWithGain ? 5 : 0),
      date,
      amount: trade.amount,
      sentence: saleWithGain
        ? `A saved sale of ${trade.ticker ?? 'an investment'} for ${formatCurrency(Math.abs(trade.amount))} on ${date} had an estimated return of ${formatPercent(trade.estimatedRealizedGainPct)}.`
        : `A saved ${trade.type.toLocaleLowerCase()} of ${trade.ticker ?? 'an investment'} for ${formatCurrency(Math.abs(trade.amount))} was posted on ${date}.`,
    })
  }

  const movements = new Map(
    data.accountMovements.map((movement) => [
      `${movement.observedAt}:${movement.accountId}`,
      movement,
    ]),
  )
  for (const change of data.lastChange?.accountChanges ?? []) {
    const observedAt = data.lastChange!.observedAt
    const id = `${observedAt}:${change.accountId}`
    if (!movements.has(id)) movements.set(id, { ...change, id, observedAt })
  }
  const eligibleMovements = [...movements.values()].filter(({ accountId, observedAt }) => {
    const date = observedAt.slice(0, 10)
    return date >= start && date <= savedDate && supportedAccount(accountId)
  })
  const pairedMovementIds = new Set<string>()
  const orderedMovements = eligibleMovements.toSorted((left, right) =>
    left.id.localeCompare(right.id),
  )
  for (const outgoing of orderedMovements.filter(({ change }) => change < 0)) {
    const incoming = orderedMovements.find(
      (candidate) =>
        candidate.change > 0 &&
        candidate.observedAt === outgoing.observedAt &&
        canonicalAccount(candidate.accountId) !== canonicalAccount(outgoing.accountId) &&
        !pairedMovementIds.has(candidate.id) &&
        Math.round(candidate.change * 100) + Math.round(outgoing.change * 100) === 0,
    )
    if (!incoming) continue
    pairedMovementIds.add(outgoing.id)
    pairedMovementIds.add(incoming.id)
  }
  const movementAmounts = eligibleMovements
    .filter(({ id }) => !pairedMovementIds.has(id))
    .map(({ change }) => Math.abs(change))
  for (const movement of eligibleMovements) {
    if (pairedMovementIds.has(movement.id)) continue
    const date = movement.observedAt.slice(0, 10)
    const isLatest =
      data.lastChange?.observedAt === movement.observedAt &&
      data.lastChange.accountChanges.some(({ accountId }) => accountId === movement.accountId)
    const account = accountById.get(movement.accountId)
    const source = [account?.institution, account?.type].filter(Boolean).join(' · ')
    contextCandidates.push({
      id: movement.id,
      kind: 'account-movement',
      score:
        30 +
        recencyPoints(date, savedDate, rangeSeconds) +
        percentilePoints(Math.abs(movement.change), movementAmounts, 20) +
        (isLatest ? 15 : 0) +
        10,
      date,
      amount: movement.change,
      sentence: `${movement.name}${source ? ` (${source})` : ''} changed by ${signedCurrency(movement.change)}.`,
    })
  }

  const tickers = [...new Set(data.holdings.map(({ ticker }) => ticker))]
  for (const ticker of tickers) {
    const detail = holdingDetail(data.holdings, ticker)
    if (
      detail.value == null ||
      detail.weeklyChange == null ||
      detail.positions.some(
        ({ accountId, currency }) =>
          Boolean(currency && currency !== 'USD') || !supportedAccount(accountId),
      )
    )
      continue
    contextCandidates.push({
      id: ticker,
      kind: 'holding',
      score:
        25 +
        Math.round(20 * clamp(Math.abs(detail.weeklyChange) / 10)) +
        Math.round(20 * Math.sqrt(clamp((detail.weight ?? 0) / 100))) +
        10,
      amount: detail.value,
      sentence: `${ticker} has a saved value of ${formatCurrency(detail.value)} and a seven-day change of ${formatPercent(detail.weeklyChange)}.`,
    })
  }

  const accounts = data.accounts.filter(
    ({ id, value, currency }) => id !== 'all' && value != null && (!currency || currency === 'USD'),
  )
  const knownAccountValue = accounts.reduce((sum, { value }) => sum + Math.max(0, value ?? 0), 0)
  for (const account of accounts) {
    contextCandidates.push({
      id: account.id,
      kind: 'account',
      score:
        10 +
        Math.round(20 * clamp(Math.max(0, account.value ?? 0) / (knownAccountValue || 1))) +
        10,
      amount: account.value ?? undefined,
      sentence: `Saved accounts include ${account.name} at ${formatCurrency(account.value)}.`,
    })
  }

  if (data.netWorthIncomplete) {
    contextCandidates.push({
      id: 'incomplete-balances',
      kind: 'warning',
      score: 50,
      sentence: 'Some saved account balances are unavailable, so the total is incomplete.',
    })
  }

  const selectedContext = rankCandidates(contextCandidates).find(({ score }) => score >= 40) ?? {
    id: 'context-gap',
    kind: 'gap' as const,
    score: 0,
    sentence: 'No recent financial detail was found in the saved period.',
  }
  const sentences: [string, string] = [selectedContext.sentence, selectedForward.sentence]
  const upcoming =
    selectedForward.kind === 'upcoming'
      ? [
          {
            sentence: selectedForward.sentence,
            direction: selectedForward.direction,
            amount: selectedForward.formattedAmount,
          },
        ]
      : []
  return {
    sentences,
    evidence: JSON.stringify({ sentences, upcoming }),
    context: {
      id: selectedContext.id,
      kind: selectedContext.kind,
      score: selectedContext.score,
    },
    forward: {
      id: selectedForward.id,
      kind: selectedForward.kind,
      score: selectedForward.score,
    },
  }
}

const normalizeNumber = (value: string) => {
  const percent = value.endsWith('%')
  return `${value.includes('$') ? '$' : ''}${Number(value.replaceAll(',', '').replace('%', '').replace('$', ''))}${percent ? '%' : ''}`
}

// Only exact evidence-backed figures and dates receive emphasis; deterministic event identity owns tone.
export function homeSummaryParts(text: string, evidence: string) {
  let upcoming: Array<{ sentence: string; direction: string; amount: string }> = []
  try {
    upcoming = JSON.parse(evidence).upcoming ?? []
  } catch {
    // Plain evidence strings are also supported by the formatter.
  }
  const sentenceKey = (sentence: string) =>
    sentence
      .replace(/[+-]?\$\d[\d,]*(?:\.\d+)?/g, normalizeNumber)
      .replace(/\s+/g, ' ')
      .trim()
  const event = upcoming.find((item) => sentenceKey(item.sentence) === sentenceKey(text))
  const pattern =
    /(?:in \d+ days?|today) \(\d{1,2}\/\d{1,2}\)|[+-]?\$\d[\d,]*(?:\.\d+)?|[+-]?\d+(?:\.\d+)?%|\b\d{4}-\d{2}-\d{2}\b|\b(?:Jan|Feb|Mar|Apr|May|Jun|Jul|Aug|Sep|Oct|Nov|Dec)[a-z]* \d{1,2}(?:, \d{4})?/g
  const key = (token: string) =>
    token.includes('$') || token.endsWith('%') ? normalizeNumber(token) : token
  const supplied = new Set((evidence.match(pattern) ?? []).map(key))
  const parts: Array<{ text: string; emphasis?: boolean; tone?: 'positive' | 'negative' }> = []
  let offset = 0
  for (const match of text.matchAll(pattern)) {
    if (!supplied.has(key(match[0]))) continue
    if (match.index > offset) parts.push({ text: text.slice(offset, match.index) })
    const token = match[0]
    const change = token.endsWith('%') ? Number.parseFloat(token) : 0
    parts.push({
      text: token,
      emphasis: true,
      tone:
        token.includes('$') && event && key(event.amount) === key(token)
          ? event.direction === 'outflow'
            ? 'negative'
            : event.direction === 'inflow'
              ? 'positive'
              : undefined
          : change > 0
            ? 'positive'
            : change < 0
              ? 'negative'
              : undefined,
    })
    offset = match.index + token.length
  }
  if (offset < text.length) parts.push({ text: text.slice(offset) })
  return parts
}
