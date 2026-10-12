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
import { formatShortDay } from '../lib/format'
import { router } from '../router'
import { resetScenario, scenario } from './routes.scenario'

// Native insights for the synthetic scenarios below, written out as fixtures. In the app
// Rust derives them; these tests check only how routes present them.
type ScenarioTransaction = import('../lib/schema').Transaction
type ScenarioActivity = import('../lib/schema').BenefitActivity
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
    engaged: false,
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
    spending: [],
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
          outcomes: [],
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
    brief: { needs: [], coming: expected.map(({ id }) => id), comingDays: 14 },
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
      classification: classification('other', { credit: true }),
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

vi.mock('../hooks/use-finance', () => ({
  useFinance: () => ({
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
      updatedAt: '2026-09-03T12:00:00Z',
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
      },
      rules: [],
      insights: scenarioInsights(scenarioTransactions()),
    },
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
})

export async function renderRoute(path: string, selectedAccount = 'selected') {
  scenario.spendingAccountId = selectedAccount
  const storage = { getItem: () => '' }
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
