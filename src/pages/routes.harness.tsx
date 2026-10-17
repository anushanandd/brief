/**
 * The route tests' shared setup: a synthetic finance scenario each test adjusts, native hooks
 * mocked to serve it, and server rendering of a route. Imported first by every routes test.
 */
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { createMemoryHistory, createRouter, RouterProvider } from '@tanstack/react-router'
import { renderToString } from 'react-dom/server'
import { afterEach, beforeEach, vi } from 'vitest'

import { loadSettingsPanel } from '../components/settings-dialog'
import { classification } from '../data/fixtures/classification'
import { emptyInsightDetails } from '../data/fixtures/insights'
import { LiveMarketProvider } from '../hooks/live-market-provider'
import { dayKey } from '../lib/dates'
import { formatShortDay, setAmountsHidden } from '../lib/format'
import { settingsSections } from '../lib/settings-search'
import { splitParts } from '../lib/transaction-kind'
import { router } from '../router'
import { scenarioReports, scenarioRollup } from './routes.reports'
import { resetScenario, scenario } from './routes.scenario'

// Native insights for the synthetic scenarios below, written out as fixtures. In the app
// Rust derives them; these tests check only how routes present them.
type ScenarioTransaction = import('../lib/schema').Transaction
type ScenarioActivity = import('../lib/schema').BenefitActivity
type ScenarioPeriod = import('../lib/schema').BenefitPeriod
/** One synthetic benefit period over a scenario window. */
const scenarioPeriod = (
  range: { windowStart: string; windowEnd: string },
  label: string,
  cap: number,
  rest: Partial<ScenarioPeriod> & Pick<ScenarioPeriod, 'state'>,
): ScenarioPeriod => ({
  start: range.windowStart,
  end: range.windowEnd,
  label,
  cap,
  credited: 0,
  activityIds: [],
  plannedTotal: scenarioCents((rest.planned ?? []).map(({ amount }) => amount)),
  ...rest,
})
const scenarioCents = (amounts: number[]) =>
  amounts.reduce((total, amount) => total + Math.round(amount * 100), 0) / 100

type ScenarioHistory = Omit<import('../lib/schema').BenefitHistory, 'years'>
/** A year of benefit periods summed as Rust sums them; estimated benefits add nothing. */
const scenarioBenefitYear = (benefits: ScenarioHistory[], year: number) => {
  const periods = benefits
    .filter(({ external }) => !external)
    .flatMap((benefit) => benefit.periods.filter(({ start }) => start.startsWith(String(year))))
  const range = (pick: (period: ScenarioPeriod) => { low: number; high: number } | undefined) => {
    const ranges = periods.flatMap((period) => pick(period) ?? [])
    return {
      low: scenarioCents(ranges.map(({ low }) => low)),
      high: scenarioCents(ranges.map(({ high }) => high)),
    }
  }
  return {
    year,
    captured: scenarioCents(periods.map(({ credited }) => credited)),
    planned: scenarioCents(periods.map(({ plannedTotal }) => plannedTotal)),
    missed: range(({ unused }) => unused),
    left: range(({ left }) => left),
  }
}
/** The card's years over the benefits shown, with what went to purchases for someone else. */
const scenarioCardYears = (
  history: ScenarioHistory[],
  activity: ScenarioActivity[],
  others: Set<string>,
  hidden: string[],
  today: string,
) =>
  [
    ...new Set(
      [today, ...history.flatMap(({ periods }) => periods.map(({ start }) => start))].map((date) =>
        Number(date.slice(0, 4)),
      ),
    ),
  ]
    .toSorted((left, right) => left - right)
    .map((year) => {
      const inYear = ({ periods }: ScenarioHistory) =>
        periods.filter(({ start }) => start.startsWith(String(year)))
      const rows = history.filter(
        (benefit) =>
          !hidden.includes(benefit.id) &&
          inYear(benefit).length > 0 &&
          (!benefit.retired || inYear(benefit).some(({ credited }) => credited !== 0)),
      )
      const forOthers = rows
        .filter(({ external }) => !external)
        .flatMap(inYear)
        .flatMap(({ activityIds }) => activityIds)
        .flatMap((id) => activity.filter((item) => item.id === id && others.has(id)))
        .filter(({ pending, kind }) => !pending && (kind === 'credit' || kind === 'reversal'))
      return {
        ...scenarioBenefitYear(rows, year),
        forOthers: scenarioCents(forOthers.map(({ amount }) => amount)),
      }
    })
// When the scenario's snapshot was saved; Rust dates Spending's periods from it.
const scenarioUpdatedAt = '2026-09-03T12:00:00Z'
type ScenarioSpendingPeriod = import('../lib/schema').SpendingPeriodJudgment
type ScenarioOwner = 'me' | 'other' | 'all'
const scenarioCounts = ({ classification: kind }: ScenarioTransaction, owner: ScenarioOwner) =>
  owner === 'me'
    ? kind.spending
    : owner === 'other'
      ? kind.othersSpending === true
      : kind.spending || kind.othersSpending === true
const sumValues = (values: Map<string, number>) => [...values.values()].reduce((a, b) => a + b, 0)
const nextDay = (date: string) => dayKey(new Date(Date.parse(`${date}T00:00:00Z`) + 86_400_000))

/**
 * Spending as Rust sums it for the scenario: this month beside last month to the same day, and
 * the open statement when there is one, for each scope and owner, each in full.
 */
function scenarioSpending(
  transactions: ScenarioTransaction[],
  today: string,
  statement?: { boundaries: string[] },
) {
  // Every account with spending, and the saved card whether or not it has any.
  const scopes = new Set([
    ...(scenario.spendingAccountId ? [scenario.spendingAccountId] : []),
    ...transactions
      .filter((transaction) => scenarioCounts(transaction, 'all'))
      .flatMap(({ accountId }) => (accountId ? [accountId] : [])),
  ])
  const owners: ScenarioOwner[] = transactions.some((transaction) =>
    scenarioCounts(transaction, 'other'),
  )
    ? ['me', 'other', 'all']
    : ['me']
  const monthStart = `${today.slice(0, 8)}01`
  const lastMonth = new Date(`${monthStart}T00:00:00Z`)
  lastMonth.setUTCMonth(lastMonth.getUTCMonth() - 1)
  const days = (Date.parse(today) - Date.parse(monthStart)) / 86_400_000
  const lastClose = statement?.boundaries.filter((date) => date < today).at(-1)
  return ['all', ...scopes].flatMap((scope) =>
    owners.map((owner) => {
      const counted = transactions.filter(
        (transaction) =>
          (scope === 'all' || transaction.accountId === scope) &&
          scenarioCounts(transaction, owner),
      )
      const spentBetween = (from: string, through: string, only?: string[]) => {
        const byCategory = new Map<string, number>()
        for (const part of counted.flatMap(splitParts)) {
          const date = (part.postedOn ?? part.date).slice(0, 10)
          if (date < from || date > through || (only && !only.includes(part.category))) continue
          const value = Math.round(Math.abs(part.amount) * 100)
          byCategory.set(part.category, (byCategory.get(part.category) ?? 0) + value)
        }
        return byCategory
      }
      // What a period lists and paces, for all of it or one category, as Rust reads it.
      const view = (start: string, end: string, focus?: string) => {
        const inPeriod = transactions.filter((transaction) => {
          const date = (transaction.postedOn ?? transaction.date).slice(0, 10)
          return (
            (scope === 'all' || transaction.accountId === scope) &&
            date >= start &&
            date <= today &&
            (owner === 'all' || (owner === 'other') === Boolean(transaction.other))
          )
        })
        const listed = inPeriod
          .filter(
            (transaction) =>
              !focus ||
              (scenarioCounts(transaction, owner) &&
                splitParts(transaction).some(({ category }) => category === focus)),
          )
          .toSorted(
            (left, right) =>
              right.date.localeCompare(left.date) || Math.abs(right.amount) - Math.abs(left.amount),
          )
        const merchants = new Map<
          string,
          { key: string; name: string; cents: number; transactionIds: string[] }
        >()
        for (const transaction of listed) {
          const key = transaction.merchant.trim().toLocaleLowerCase()
          const group = merchants.get(key) ?? {
            key,
            name: transaction.merchant,
            cents: 0,
            transactionIds: [],
          }
          group.cents += Math.round(transaction.amount * 100)
          group.transactionIds.push(transaction.id)
          merchants.set(key, group)
        }
        const spentOn = (date: string) =>
          spentBetween(date, date, focus === undefined ? undefined : [focus])
        const dates = Array.from(
          { length: (Date.parse(today) - Date.parse(start)) / 86_400_000 + 1 },
          (_, index) => dayKey(new Date(Date.parse(`${start}T00:00:00Z`) + index * 86_400_000)),
        )
        let running = 0
        return {
          transactionIds: listed.map(({ id }) => id),
          merchants: [...merchants.values()]
            .toSorted((left, right) => left.cents - right.cents)
            .map(({ cents, ...group }) => ({ ...group, total: cents / 100 })),
          pace: {
            start,
            end,
            current: dates.map((date) => (running += sumValues(spentOn(date))) / 100),
            previous: [],
            projected: null,
            days: [],
          },
        }
      }
      const period = (
        kind: ScenarioSpendingPeriod['kind'],
        start: string,
        end: string,
        before?: [string, string],
      ) => {
        const current = spentBetween(start, today)
        const previous = before ? spentBetween(...before) : undefined
        const total = sumValues(current)
        const earlier = previous && sumValues(previous)
        return {
          ...view(start, end),
          earlier: false,
          bars: [],
          focus: Object.fromEntries(
            [...current.keys()].map((name) => [name, view(start, end, name)]),
          ),
          kind,
          start,
          end: today,
          total: total / 100,
          previous: earlier == null ? null : earlier / 100,
          change: earlier == null ? null : (total - earlier) / 100,
          changePct: earlier ? Math.round(((total - earlier) / earlier) * 10_000) / 100 : null,
          daily: 0,
          pending: 0,
          categories: [...current]
            .toSorted(([, left], [, right]) => right - left)
            .map(([name, value]) => ({
              name,
              total: value / 100,
              previous: previous ? (previous.get(name) ?? 0) / 100 : null,
              change: previous ? (value - (previous.get(name) ?? 0)) / 100 : null,
              changePct: null,
              daily: 0,
              share: total ? Math.round((value / total) * 10_000) / 100 : 0,
              typical: null,
              aboveTypical: null,
            })),
          ...(kind === 'statement' && owner !== 'other'
            ? { opening: 0, balance: total / 100 }
            : {}),
        }
      }
      const lastMonthStart = dayKey(lastMonth)
      const lastMonthThrough = dayKey(
        new Date(
          Math.min(
            Date.parse(`${lastMonthStart}T00:00:00Z`) + days * 86_400_000,
            Date.parse(`${monthStart}T00:00:00Z`) - 86_400_000,
          ),
        ),
      )
      const nextClose = statement?.boundaries.find((date) => date >= today) ?? today
      const monthEnd = dayKey(
        new Date(Date.UTC(lastMonth.getUTCFullYear(), lastMonth.getUTCMonth() + 2, 0)),
      )
      return {
        scope,
        owner,
        // Each current period in full, as `useSpendingPeriod` reads it.
        periods: [
          ...(lastClose ? [period('statement', nextDay(lastClose), nextClose)] : []),
          period('month', monthStart, monthEnd, [lastMonthStart, lastMonthThrough]),
        ],
        months: [],
      }
    }),
  )
}

function scenarioInsights(transactions: ScenarioTransaction[]): import('../lib/schema').Insights {
  const now = new Date()
  const today = now.toISOString().slice(0, 10)
  const year = now.getUTCFullYear()
  const month = now.getUTCMonth()
  const window = (startMonth: number, months: number) => ({
    windowStart: dayKey(new Date(Date.UTC(year, startMonth, 1))),
    windowEnd: dayKey(new Date(Date.UTC(year, startMonth + months, 0))),
  })
  const daysRemaining = (end: string) =>
    Math.max(0, Math.ceil((Date.parse(end) - Date.parse(today)) / 86_400_000))
  const card = scenario.spendingAccountId
  const activity: ScenarioActivity[] = transactions
    .filter(({ accountId }) => accountId === card)
    .flatMap((transaction): ScenarioActivity[] => {
      const date = (transaction.postedOn ?? transaction.date).slice(0, 10)
      const base = {
        id: transaction.id,
        merchant: transaction.merchant,
        description: transaction.description ?? undefined,
        amount: transaction.amount,
        pending: transaction.pending,
        accountId: transaction.accountId,
        date,
      }
      if (/resy credit/i.test(transaction.merchant))
        return [
          {
            ...base,
            benefitId: 'resy',
            benefitName: 'Resy',
            kind: transaction.amount > 0 ? ('credit' as const) : ('reversal' as const),
          },
        ]
      if (transaction.id === 'uber-purchase')
        return [
          { ...base, benefitId: 'uber-cash', benefitName: 'Uber Cash', kind: 'purchase' as const },
        ]
      return []
    })
    .toSorted((left, right) => right.date.localeCompare(left.date))
  const inWindow = (id: string, start: string, end: string) =>
    activity.filter(({ benefitId, date }) => benefitId === id && date >= start && date <= end)
  const quarter = window(Math.floor(month / 3) * 3, 3)
  const resyActivity = inWindow('resy', quarter.windowStart, quarter.windowEnd)
  const posted = resyActivity.filter(({ pending }) => !pending)
  const credited = posted.reduce((sum, { amount }) => sum + Math.round(amount * 100), 0) / 100
  const lastCredit = activity.find(
    ({ benefitId, kind, pending, date }) =>
      benefitId === 'resy' && kind === 'credit' && !pending && date < quarter.windowStart,
  )
  const monthly = window(month, 1)
  const uberActivity = inWindow('uber-cash', monthly.windowStart, monthly.windowEnd)
  const benefit = (
    id: string,
    name: string,
    limit: string,
    cap: number,
    cadence: 'monthly' | 'quarterly' | 'renewal',
    range: { windowStart: string; windowEnd: string },
  ) => ({
    id,
    name,
    limit,
    cap,
    cadence,
    instruction: `${name} terms.`,
    url: 'https://example.com/terms',
    periodUncertain: false,
    snapshotBeforeWindow: false,
    postingDelayDays: 56,
    creditLagDays: 56,
    creditLagLearned: false,
    status: 'unused' as const,
    recurring: false,
    creditedAmount: 0,
    uncertainCreditAmount: 0,
    awaitingCreditAmount: 0,
    estimatedCreditAmount: null as number | null,
    remainingAmount: cap as number | null,
    remainingRange: { low: cap, high: cap } as { low: number; high: number } | undefined,
    activity: [] as typeof activity,
    ...range,
    windowLabel: `${formatShortDay(range.windowStart)} – ${formatShortDay(range.windowEnd)}, ${year}`,
    daysRemaining: daysRemaining(range.windowEnd),
    reset: formatShortDay(range.windowEnd),
  })
  const reversed = posted.some(({ kind }) => kind === 'reversal')
  const windows = [
    {
      ...benefit(
        'digital-entertainment',
        'Digital entertainment',
        '$25 / month',
        25,
        'monthly',
        monthly,
      ),
    },
    {
      ...benefit('uber-cash', 'Uber Cash', '$15 monthly · +$20 Dec', 15, 'monthly', monthly),
      status: 'external' as const,
      remainingAmount: null,
      remainingRange: undefined,
      estimatedCreditAmount: uberActivity.some(({ pending }) => !pending) ? 15 : 0,
      activity: uberActivity,
    },
    {
      ...benefit('resy', 'Resy', '$100 / quarter', 100, 'quarterly', quarter),
      status: credited > 0 ? ('credited' as const) : ('unused' as const),
      creditedAmount: credited,
      periodUncertain: reversed,
      remainingAmount: reversed ? null : Math.max(0, 100 - credited),
      remainingRange: reversed
        ? { low: Math.max(0, 100 - credited), high: 100 }
        : { low: Math.max(0, 100 - credited), high: Math.max(0, 100 - credited) },
      activity: resyActivity,
      ...(lastCredit
        ? {
            lastCredit: {
              amount: lastCredit.amount,
              date: new Date(`${lastCredit.date}T00:00:00Z`).toLocaleDateString('en-US', {
                month: 'short',
                day: 'numeric',
                year: 'numeric',
                timeZone: 'UTC',
              }),
            },
          }
        : {}),
    },
    {
      ...benefit(
        'global-entry',
        'Global Entry / TSA PreCheck',
        '$120 / up to $85',
        120,
        'renewal',
        window(0, 12),
      ),
      remainingAmount: null,
      remainingRange: undefined,
    },
  ].toSorted(
    (left, right) =>
      Number(left.cadence === 'renewal') - Number(right.cadence === 'renewal') ||
      left.windowEnd.localeCompare(right.windowEnd) ||
      left.name.localeCompare(right.name),
  )
  // Each benefit's periods this year: earlier periods settled, the current one open.
  const resyWindow = windows.find(({ id }) => id === 'resy')!
  const monthNames = [
    'Jan',
    'Feb',
    'Mar',
    'Apr',
    'May',
    'Jun',
    'Jul',
    'Aug',
    'Sep',
    'Oct',
    'Nov',
    'Dec',
  ]
  const history = [
    {
      id: 'resy',
      name: 'Resy',
      recurring: false,
      external: false,
      retired: false,
      periods: [0, 1, 2, 3].map((quarterIndex) => {
        const current = Math.floor(month / 3)
        const range = window(quarterIndex * 3, 3)
        const label = `Q${quarterIndex + 1}`
        if (quarterIndex > current) return scenarioPeriod(range, label, 100, { state: 'future' })
        if (quarterIndex === current)
          return scenarioPeriod(range, label, 100, {
            state: 'open',
            credited,
            left: resyWindow.remainingRange,
            activityIds: resyActivity.map(({ id }) => id),
          })
        return quarterIndex === current - 1
          ? scenarioPeriod(range, label, 100, {
              state: 'partial',
              credited: 40,
              unused: { low: 60, high: 60 },
            })
          : scenarioPeriod(range, label, 100, { state: 'missed', unused: { low: 100, high: 100 } })
      }),
    },
    {
      id: 'uber-cash',
      name: 'Uber Cash',
      recurring: false,
      external: true,
      retired: false,
      periods: monthNames.map((label, index) =>
        scenarioPeriod(window(index, 1), label, 15, {
          state:
            index < month
              ? 'not-visible'
              : index > month
                ? 'future'
                : uberActivity.some(({ pending }) => !pending)
                  ? 'likely-used'
                  : 'open',
          activityIds: index === month ? uberActivity.map(({ id }) => id) : [],
        }),
      ),
    },
    {
      id: 'digital-entertainment',
      name: 'Digital entertainment',
      recurring: true,
      external: false,
      retired: false,
      periods: monthNames.map((label, index) =>
        scenarioPeriod(
          window(index, 1),
          label,
          25,
          index < month
            ? { state: 'full', credited: 25, unused: { low: 0, high: 0 } }
            : index === month
              ? { state: 'open', left: { low: 25, high: 25 } }
              : {
                  state: 'future',
                  // A streaming subscription the forecast expects each month.
                  planned: [
                    {
                      title: 'Example streaming',
                      date: window(index, 1).windowStart,
                      amount: 13.99,
                      source: 'recurring' as const,
                    },
                  ],
                },
        ),
      ),
    },
  ]
  // A positive Resy amount without the issuer's wording: a credit or a return.
  const review = transactions.some(({ id }) => id === 'transaction-1')
    ? [
        {
          kind: 'unverified-credit' as const,
          transactionId: 'transaction-1',
          benefitId: 'resy',
          periodStart: window(Math.floor(month / 3) * 3, 3).windowStart,
        },
      ]
    : []
  const series = new Map<string, ScenarioTransaction[]>()
  for (const transaction of transactions.filter(({ pending }) => !pending)) {
    const key = transaction.recurringId ?? ''
    series.set(key, [...(series.get(key) ?? []), transaction])
  }
  const expected = [...series].flatMap(([recurringId, members]) => {
    if (!recurringId.startsWith('series:') || members.length < 3) return []
    const last = members.toSorted((left, right) => left.date.localeCompare(right.date)).at(-1)!
    const next = new Date(`${last.date}T00:00:00Z`)
    next.setUTCMonth(next.getUTCMonth() + 1)
    return [
      {
        id: recurringId,
        recurringId,
        title: last.merchant,
        kind: last.amount > 0 ? ('income' as const) : ('subscription' as const),
        date: dayKey(next),
        amount: last.amount,
        amountLow: Math.abs(last.amount),
        amountHigh: Math.abs(last.amount),
        frequency: 'Monthly',
        observations: members.length,
        lastSeen: last.date,
        state: 'expected' as const,
        accountId: last.accountId,
        sourceTransactionId: last.id,
      },
    ]
  })
  return {
    today,
    computedAt: now.toISOString(),
    expected,
    benefits: card
      ? {
          accountId: card,
          product: 'amex-platinum',
          productName: 'American Express Platinum',
          activity,
          windows,
          useFirst: windows
            .filter(({ remainingRange }) => (remainingRange?.high ?? 0) > 0)
            .map(({ id }) => id),
          history: history.map((entry) => ({
            ...entry,
            years: [...new Set(entry.periods.map(({ start }) => Number(start.slice(0, 4))))].map(
              (periodYear) => scenarioBenefitYear([entry], periodYear),
            ),
          })),
          years: scenarioCardYears(
            history,
            activity,
            new Set(transactions.filter(({ other }) => other).map(({ id }) => id)),
            scenario.hiddenBenefits.split(',').filter(Boolean),
            today,
          ),
          review,
        }
      : undefined,
    benefitProducts: [{ id: 'amex-platinum', name: 'American Express Platinum' }],
    benefitOptions: windows.map(({ id, name }) => ({ id, name })),
    benefitRulesVersion: '2026-09-20',
    benefitTermsReviewed: 'Sep 20, 2026',
    subscriptions: { monthly: 0, items: [] },
    refundSuggestions: [],
    plan: {
      monthStart: dayKey(new Date(Date.UTC(year, month, 1))),
      daysElapsed: now.getUTCDate(),
      daysInMonth: new Date(Date.UTC(year, month + 1, 0)).getUTCDate(),
      spent: 0,
      projected: 0,
      categories: [],
      coverage: {
        cash: 0,
        cashIncomplete: false,
        expectedOutflows: 0,
        expectedInflows: 0,
        afterOutflows: 0,
        horizonDays: 30,
      },
    },
    attention: [],
    brief: { needs: [] },
    cash: {
      horizonDays: 90,
      cash: 0,
      pendingChange: 0,
      cashIncomplete: true,
      days: [],
      events: [],
    },
    ...emptyInsightDetails,
    returns: [],
    periods: [],
    recap: [],
    changes: [],
    interest: [],
    lots: [],
    longTermSoon: [],
    investmentAttention: { flags: [], earningsWatch: [] },
    securities: [],
    spending: scenarioSpending(
      transactions,
      scenarioUpdatedAt.slice(0, 10),
      scenario.insights.statement ?? undefined,
    ),
    ...scenario.insights,
  }
}

function scenarioTransactions(): ScenarioTransaction[] {
  return [
    ...Array.from({ length: 7 }, (_, index) => ({
      id: `transaction-${index}`,
      ...(index === 0 && scenario.postingDate ? { postedOn: scenario.postingDate } : {}),
      ...(index === 0 && scenario.activityMethod
        ? { paymentChannel: scenario.activityMethod }
        : {}),
      ...(scenario.linkedPending && index === 0
        ? { website: 'https://example.com/', description: 'Synthetic provider description' }
        : {}),
      merchant: `Resy credit ${index}`,
      category: 'Credit',
      date: `2026-09-0${(index % 3) + 1}`,
      amount: scenario.benefitAmount,
      account: 'Selected card',
      accountId: 'selected',
      classification:
        scenario.creditKind === 'income'
          ? classification('income')
          : classification('other', { credit: true }),
      pending: scenario.linkedPending && index === 0,
    })),
    ...(scenario.uberPurchase
      ? [
          {
            id: 'uber-purchase',
            merchant: 'Uber',
            category: 'Travel',
            date: '2026-09-02',
            amount: -5,
            account: 'Selected card',
            accountId: 'selected',
            classification: classification('expense'),
            pending: false,
          },
        ]
      : []),
    {
      id: 'other',
      merchant: scenario.otherMerchant,
      category: scenario.otherCategory,
      date: '2026-09-01',
      amount: 900,
      account: 'Other card',
      accountId: 'other',
      classification: classification(scenario.otherKind),
      pending: false,
    },
    ...scenario.extraTransactions.map((transaction) => ({
      ...transaction,
      recurringId: `series:${transaction.accountId}:${transaction.merchant}`,
    })),
  ]
}

vi.mock('../hooks/use-finance', () => ({ useFinance: () => scenarioFinance() }))

/** The scenario's finance as `useFinance` serves it, with Rust's totals over its accounts. */
function scenarioFinance() {
  const finance = {
    integrationStatus: { plaid: false, snaptrade: false, alpaca: false },
    marketSymbols: ['SYNTH'],
    runtimeLog: [],
    annotationWarning: scenario.annotationWarning,
    data: {
      recovery: scenario.recovery,
      netWorthIncomplete: scenario.incomplete,
      providerStatus: scenario.providerError
        ? { plaid: { updatedAt: null, error: 'Synthetic provider error' } }
        : {},
      updatedAt: scenarioUpdatedAt,
      netWorth: scenario.snaptradeReference ? 300 : -2_100,
      benchmarkHistory: [],
      netWorthHistory: [{ date: '2026-09-03', value: scenario.snaptradeReference ? 300 : -2_100 }],
      spending: { monthTotal: 0, categories: [] },
      accounts: [
        {
          id: scenario.snaptradeReference ? 'snaptrade:selected' : 'selected',
          name: 'Selected card',
          institution: 'Test Bank',
          type: scenario.accountType,
          value: scenario.snaptradeReference ? 1_200 : -1_200,
          ...(scenario.snaptradeReference
            ? {
                balanceSource: 'cash-and-positions',
                cashValue: 200,
                investedValue: 1_000,
                reportedBalance: 900,
                balanceAsOf: '2026-09-02T20:00:00Z',
              }
            : {}),
          costBasisCoverage: scenario.partialBasis ? 'partial' : 'unavailable',
          knownCostBasis: scenario.partialBasis ? 100 : null,
          knownUnrealizedGain: scenario.partialBasis ? 0 : null,
        },
        {
          id: 'other',
          name: 'Other card',
          institution: 'Other Bank',
          type: scenario.otherAccountType,
          value: -900,
        },
      ],
      transactions: scenarioTransactions(),
      trades: scenario.trades,
      accountMovements: [],
      transactionChanges: [],
      accountBalanceHistory: [
        {
          accountId: 'other',
          name: 'Other card',
          institution: 'Other Bank',
          currentValue: -900,
          historySource: 'transaction-derived',
          historyStart: '2026-09-01',
          performanceMethod: 'value-only',
          points: [
            { date: '2026-09-01', value: -800, netDeposits: null, sp500: null },
            { date: '2026-09-03', value: -900, netDeposits: null, sp500: null },
          ],
        },
        {
          accountId: 'selected',
          name: 'Selected card',
          institution: 'Test Bank',
          currentValue: -1_200,
          historySource: 'transaction-derived',
          historyStart: '2026-09-01',
          performanceMethod: 'value-only',
          points: [
            ...(scenario.singleHistory
              ? []
              : [{ date: '2026-09-01', value: -1_100, netDeposits: null, sp500: null }]),
            { date: '2026-09-03', value: -1_200, netDeposits: null, sp500: null },
          ],
        },
      ],
      brokeragePerformance: scenario.homeHistory
        ? [
            {
              accountId: 'selected',
              name: 'Selected card',
              institution: 'Test Bank',
              currentValue: -1_200,
              points: [
                { date: '2026-09-01', value: -1_100, netDeposits: -1_100, sp500: -1_100 },
                { date: '2026-09-03', value: -1_200, netDeposits: -1_150, sp500: -1_050 },
              ],
            },
          ]
        : [],
      holdings: scenario.holdings,
      settings: {
        spendingAccountId: scenario.spendingAccountId || undefined,
        hiddenBenefitIds: scenario.hiddenBenefits.split(',').filter(Boolean),
        accountNames: {},
        accountStartDates: {},
        accountClosedDates: {},
        manualAccounts: [],
        people: scenario.people,
        tripCompanions: scenario.tripCompanions,
      },
      insights: scenarioInsights(scenarioTransactions()),
      documents: scenario.documents,
    },
  }
  const { data } = finance
  return {
    ...finance,
    data: {
      ...data,
      insights: {
        ...data.insights,
        rollup:
          scenario.insights.rollup ??
          scenarioRollup(data.accounts, data.holdings, data.netWorth, data.settings),
      },
    },
  }
}
// Rust reads a Spending period on demand; here the scenario's current periods carry it in full.
vi.mock('../hooks/use-spending-period', () => ({
  useSpendingPeriod: (
    data: import('../lib/schema').FinanceSnapshot | undefined,
    request: import('../lib/api').SpendingPeriodRequest,
  ) => {
    const periods =
      data?.insights?.spending.find(
        ({ scope, owner }) => scope === request.scope && owner === request.owner,
      )?.periods ?? []
    const period =
      request.offset === 0
        ? (periods.find(({ kind }) => kind === request.kind) ??
          (request.kind === 'statement' ? periods.find(({ kind }) => kind === 'month') : undefined))
        : undefined
    const empty = { start: period?.start ?? '', end: period?.end ?? '' }
    return {
      data: period
        ? {
            earlier: false,
            transactionIds: [],
            merchants: [],
            pace: { ...empty, current: [], previous: [], projected: null, days: [] },
            bars: [],
            focus: {},
            ...period,
          }
        : null,
      isLoading: false,
      isError: false,
      isPlaceholderData: false,
    }
  },
}))
// Rust reads reports on demand; here they follow the same rules over the scenario.
vi.mock('../hooks/use-reports', () => ({
  useReports: (
    data: import('../lib/schema').FinanceSnapshot | undefined,
    request: import('../lib/api').ReportRequest,
    enabled = true,
  ) => ({
    data: enabled && data ? { ...scenarioReports(data, request), ...scenario.reports } : undefined,
    isLoading: false,
    isError: false,
    isPlaceholderData: false,
  }),
}))
vi.mock('../hooks/use-refresh-finance', () => ({
  useAutoRefreshFinance: () => undefined,
  useRefreshFinance: () => () => Promise.resolve(),
  useFinanceRefreshState: () => false,
}))

// Server rendering checks route output, not mounted effects or user interactions.
beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date('2026-09-16T12:00:00Z'))
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
  resetScenario()
  setAmountsHidden(false)
})

export async function renderRoute(path: string, selectedAccount = 'selected') {
  scenario.spendingAccountId = selectedAccount
  const storage = {
    getItem: (key: string) =>
      key === 'brief:hide-values-on-launch' && scenario.valuesHidden ? 'true' : '',
  }
  vi.stubGlobal('window', { localStorage: storage })
  vi.stubGlobal('localStorage', storage)
  const testRouter = createRouter({
    routeTree: router.routeTree,
    history: createMemoryHistory({ initialEntries: [path] }),
  })
  await testRouter.load()
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: Infinity } },
  })
  try {
    return renderToString(
      <QueryClientProvider client={client}>
        <LiveMarketProvider>
          <RouterProvider router={testRouter} />
        </LiveMarketProvider>
      </QueryClientProvider>,
    )
  } finally {
    client.clear()
  }
}

/** Settings loads lazily inside its dialog; render once to start the load, then again. */
export async function renderSettings(path = '/#settings') {
  await renderRoute(path)
  await loadSettingsPanel()
  await new Promise((resolve) => setTimeout(resolve, 0))
  return renderRoute(path)
}

/** Settings renders one section at a time, so this opens each section by its address. */
export async function renderEverySettingsSection(page = '/') {
  const sections: string[] = []
  for (const { id } of settingsSections)
    sections.push(await renderSettings(`${page}#settings/${id}`))
  return sections.join('\n')
}

/** Whether the app has a route at `path`. */
export const hasRoute = (path: string) => path in router.routesByPath

export function redirectOf(id: keyof typeof router.routesById, context: object) {
  try {
    Reflect.apply(router.routesById[id].options.beforeLoad!, undefined, [context])
  } catch (error) {
    return error
  }
  return undefined
}
