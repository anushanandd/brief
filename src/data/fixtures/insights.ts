import type {
  BenefitActivity,
  BenefitWindow,
  ExpectedEvent,
  Insights,
  Rollup,
} from '../../lib/schema'

/** Empty results for the insights a scenario doesn't exercise. */
export const emptyInsightDetails = {
  credit: { cards: [], balance: 0, limit: 0, cardsWithoutLimit: 0 },
  anomalies: { duplicates: [], unusual: [] },
  trips: { trips: [], years: [], home: [] },
  people: { people: [], sent: 0, suggestions: [] },
  recurringCheck: { available: false, ended: [], unforecast: [] },
  taxes: {
    years: [],
    harvest: [],
    washWindows: [],
    hasProfile: false,
    tablesVersion: '',
    sources: [],
  },
  dividends: { securities: [], annualIncome: 0, upcoming: [] },
  retirement: {
    year: 2026,
    ira: [],
    deposits: [],
    buckets: { preTax: 0, taxFree: 0, taxable: 0, complete: true },
    rothBasis: [],
    savings: [],
    uncounted: [],
  },
  documents: { years: [], pay: [], keep: [], toReview: 0, outdated: [], evidence: [] },
  evidence: {
    explanations: [],
    alerts: [],
    chapters: [],
    now: {},
    residency: [],
    programs: [],
  },
  agenda: [],
  flows: [],
  activityDays: [],
  activityMonths: [],
  rollup: emptyRollup(),
  chartChanges: [],
} satisfies Partial<Insights>

/** Account totals with nothing in them; a test names the ones it exercises. */
export function emptyRollup(overrides: Partial<Rollup> = {}): Rollup {
  return {
    groups: [],
    listed: [],
    shares: [],
    sheet: { assets: 0, liabilities: 0, complete: true },
    unrealized: null,
    realizedYtd: null,
    retirement: null,
    netWorth: 0,
    combined: null,
    holdings: [],
    ...overrides,
  }
}

/** A native insights payload for renderer tests. Rust computes these in the app. */
export function insights(overrides: Partial<Insights> = {}): Insights {
  return {
    today: '2026-09-18',
    computedAt: '2026-09-18T12:00:00Z',
    expected: [],
    spending: [],
    benefitProducts: [{ id: 'amex-platinum', name: 'American Express Platinum' }],
    benefitOptions: [
      { id: 'resy', name: 'Resy' },
      { id: 'uber-cash', name: 'Uber Cash' },
    ],
    benefitRulesVersion: '2026-09-20',
    benefitTermsReviewed: 'Sep 20, 2026',
    subscriptions: { monthly: 0, items: [] },
    refundSuggestions: [],
    plan: {
      monthStart: '2026-09-01',
      daysElapsed: 18,
      daysInMonth: 30,
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
    ...overrides,
  }
}

export function expectedEvent(overrides: Partial<ExpectedEvent> = {}): ExpectedEvent {
  return {
    id: 'series',
    recurringId: 'series',
    title: 'Synthetic series',
    kind: 'payment',
    date: '2026-09-20',
    amount: -15,
    amountLow: 15,
    amountHigh: 15,
    frequency: 'Monthly',
    observations: 3,
    lastSeen: '2026-08-20',
    state: 'expected',
    sourceTransactionId: 'source',
    ...overrides,
  }
}

export function benefitActivity(overrides: Partial<BenefitActivity> = {}): BenefitActivity {
  return {
    id: 'credit',
    merchant: 'Resy credit',
    amount: 10,
    pending: false,
    accountId: 'selected',
    date: '2026-09-01',
    benefitId: 'resy',
    benefitName: 'Resy',
    kind: 'credit',
    ...overrides,
  }
}

export function benefitWindow(overrides: Partial<BenefitWindow> = {}): BenefitWindow {
  return {
    id: 'resy',
    name: 'Resy',
    limit: '$100 / quarter',
    cap: 100,
    cadence: 'quarterly',
    instruction: 'Synthetic instruction.',
    url: 'https://example.com/terms',
    periodUncertain: false,
    snapshotBeforeWindow: false,
    postingDelayDays: 56,
    creditLagDays: 7,
    creditLagLearned: false,
    status: 'unused',
    recurring: false,
    creditedAmount: 0,
    uncertainCreditAmount: 0,
    awaitingCreditAmount: 0,
    estimatedCreditAmount: null,
    remainingAmount: 100,
    remainingRange: { low: 100, high: 100 },
    activity: [],
    windowStart: '2026-07-01',
    windowEnd: '2026-09-30',
    windowLabel: 'Jul 1 – Sep 30, 2026',
    daysRemaining: 12,
    reset: 'Sep 30',
    ...overrides,
  }
}
