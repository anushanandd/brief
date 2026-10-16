import {
  createRootRoute,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
} from '@tanstack/react-router'

import { AppShell } from './components/app-shell'
import type { LedgerSearch } from './lib/activity-ledger'
import { analyticsChart, analyticsSearch } from './lib/analytics'
import { calendarSearch } from './lib/calendar'
import { validDate } from './lib/dates'
import { insightsSearch } from './lib/insights-views'
import { settingsHash } from './lib/settings-dialog'
import { settingsHashForPath } from './lib/settings-search'
import { spendingRanges, type SpendingRange } from './lib/spending'

const rootRoute = createRootRoute({ component: AppShell })

const homeRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: lazyRouteComponent(() => import('./pages/overview'), 'OverviewPage'),
})
// Overview became Home; keep old links working.
const overviewRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/overview',
  beforeLoad: () => {
    throw redirect({ to: '/' })
  },
})
const accountSearch = (search: Record<string, unknown>): { account?: string } =>
  typeof search.account === 'string' && search.account ? { account: search.account } : {}
const accountsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/accounts',
  validateSearch: accountSearch,
  component: lazyRouteComponent(() => import('./pages/accounts'), 'AccountsPage'),
})
/** A non-negative amount from a search parameter. */
const amountBound = (value: unknown) => {
  const amount = typeof value === 'number' ? value : typeof value === 'string' ? Number(value) : NaN
  return Number.isFinite(amount) && amount >= 0 ? amount : undefined
}
const evidenceValues = ['statement', 'unlisted', 'conflict', 'unsure'] as const
const ledgerSearch = (search: Record<string, unknown>): LedgerSearch => {
  const from = validDate(search.from)
  const to = validDate(search.to)
  const min = amountBound(search.min)
  const max = amountBound(search.max)
  const evidence = evidenceValues.find((value) => value === search.evidence)
  return {
    ...(analyticsChart(search.analysis) ? { analysis: analyticsChart(search.analysis) } : {}),
    ...(typeof search.account === 'string' && search.account ? { account: search.account } : {}),
    ...(typeof search.category === 'string' && search.category
      ? { category: search.category }
      : {}),
    ...(typeof search.exclude === 'string' && search.exclude ? { exclude: search.exclude } : {}),
    ...(typeof search.method === 'string' && search.method ? { method: search.method } : {}),
    ...(search.for === 'me' || search.for === 'other' ? { for: search.for } : {}),
    ...(evidence ? { evidence } : {}),
    ...(typeof search.q === 'string' && search.q.trim() ? { q: search.q } : {}),
    ...(from ? { from } : {}),
    ...(to ? { to } : {}),
    ...(min != null ? { min } : {}),
    ...(max != null ? { max } : {}),
    ...(typeof search.open === 'string' && search.open ? { open: search.open } : {}),
    ...(search.review === true || search.review === 'true' || search.review === 1
      ? { review: true as const }
      : {}),
  }
}
// Investments is one split view: the layout stays mounted while the selected holding changes,
// so the list keeps its scroll position and sort.
const holdingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/holdings',
  // A holding once lived at `?ticker=`; keep those links working.
  validateSearch: (search: Record<string, unknown>): { ticker?: string } =>
    typeof search.ticker === 'string' && search.ticker ? { ticker: search.ticker } : {},
  beforeLoad: ({ search }) => {
    if (search.ticker)
      throw redirect({ to: '/holdings/$ticker', params: { ticker: search.ticker }, replace: true })
  },
  component: lazyRouteComponent(() => import('./pages/holdings'), 'HoldingsPage'),
})
const holdingsIndexRoute = createRoute({ getParentRoute: () => holdingsRoute, path: '/' })
const performanceRoute = createRoute({
  getParentRoute: () => holdingsRoute,
  path: 'performance',
  beforeLoad: () => {
    throw redirect({ to: '/holdings', replace: true })
  },
})
// A fixed segment outranks `$ticker`, so no holding can take this address.
const retirementRoute = createRoute({ getParentRoute: () => holdingsRoute, path: 'retirement' })
const securityRoute = createRoute({ getParentRoute: () => holdingsRoute, path: '$ticker' })
const activitiesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities',
  validateSearch: ledgerSearch,
  component: lazyRouteComponent(() => import('./pages/activities'), 'ActivitiesPage'),
})
const spendingRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/spending',
  validateSearch: (
    search: Record<string, unknown>,
  ): { account?: string; range?: SpendingRange; for?: 'other' | 'all' } => {
    const range = spendingRanges.find((item) => item === search.range)
    // Your own spending is the default, so it never appears in the URL.
    const owner = search.for === 'other' || search.for === 'all' ? search.for : undefined
    return {
      ...accountSearch(search),
      ...(range ? { range } : {}),
      ...(owner ? { for: owner } : {}),
    }
  },
  component: lazyRouteComponent(() => import('./pages/spending'), 'SpendingPage'),
})
const insightsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/insights',
  validateSearch: insightsSearch,
  component: lazyRouteComponent(() => import('./pages/insights'), 'InsightsPage'),
})
const calendarRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/calendar',
  validateSearch: calendarSearch,
  component: lazyRouteComponent(() => import('./pages/calendar'), 'CalendarPage'),
})
const documentsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/documents',
  validateSearch: (search: Record<string, unknown>): { year?: number; document?: string } => {
    const year = Number(search.year)
    return {
      ...(Number.isInteger(year) && year > 1990 && year < 2200 ? { year } : {}),
      ...(typeof search.document === 'string' && /^[0-9a-f]{64}$/.test(search.document)
        ? { document: search.document }
        : {}),
    }
  },
  component: lazyRouteComponent(() => import('./pages/documents'), 'DocumentsPage'),
})
// Trips moved under Insights; keep old links working.
const tripsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/trips',
  validateSearch: insightsSearch,
  beforeLoad: ({ search }) => {
    throw redirect({ to: '/insights', search })
  },
})
// Subscriptions moved onto Spending; keep old links working.
const recurringRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/recurring',
  beforeLoad: () => {
    throw redirect({ to: '/spending', replace: true })
  },
})
const platinumBenefitsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/spending/platinum',
  beforeLoad: () => {
    throw redirect({ to: '/analytics', search: { chart: 'amex-credits' } })
  },
})
const moneyRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/money',
  beforeLoad: () => {
    throw redirect({ to: '/analytics' })
  },
})
const analyticsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/analytics',
  validateSearch: analyticsSearch,
  component: lazyRouteComponent(() => import('./pages/analytics'), 'AnalyticsPage'),
})
const spendingActivityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/spending',
  beforeLoad: () => {
    throw redirect({ to: '/spending' })
  },
})
// Settings is a dialog addressed by the hash (`#settings/privacy`) over any page. These
// paths remain for older links and native health checks, which still emit them.
const taxesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/taxes',
  validateSearch: (search: Record<string, unknown>): { year?: number } => {
    const year = typeof search.year === 'number' ? search.year : Number(search.year)
    return Number.isInteger(year) && year > 2000 && year < 2100 ? { year } : {}
  },
  component: lazyRouteComponent(() => import('./pages/taxes'), 'TaxesPage'),
})
const settingsRedirect = (path: string) => {
  throw redirect({
    to: '/',
    hash: settingsHash(settingsHashForPath(path)),
    replace: true,
  })
}
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  beforeLoad: ({ location }) =>
    settingsRedirect(`/settings${location.hash ? `#${location.hash}` : ''}`),
})
const legacySettingsRoutes = (
  ['appearance', 'accounts', 'connections', 'data', 'advanced'] as const
).map((page) =>
  createRoute({
    getParentRoute: () => rootRoute,
    path: `/settings/${page}`,
    beforeLoad: ({ location }) =>
      settingsRedirect(`/settings/${page}${location.hash ? `#${location.hash}` : ''}`),
  }),
)
const designRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings/design',
  component: lazyRouteComponent(() => import('./pages/design'), 'DesignPage'),
})
const logsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/logs',
  component: lazyRouteComponent(() => import('./pages/logs'), 'LogsPage'),
})
const healthRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/health',
  component: lazyRouteComponent(() => import('./pages/health'), 'HealthPage'),
})

const routeTree = rootRoute.addChildren([
  homeRoute,
  overviewRoute,
  accountsRoute,
  holdingsRoute.addChildren([holdingsIndexRoute, performanceRoute, retirementRoute, securityRoute]),
  taxesRoute,
  activitiesRoute,
  spendingRoute,
  recurringRoute,
  platinumBenefitsRoute,
  insightsRoute,
  calendarRoute,
  documentsRoute,
  tripsRoute,
  moneyRoute,
  analyticsRoute,
  spendingActivityRoute,
  settingsRoute,
  ...legacySettingsRoutes,
  // Design references and prototypes are development tools, never part of a release build.
  ...(import.meta.env.DEV ? [designRoute] : []),
  logsRoute,
  healthRoute,
])

export const router = createRouter({ routeTree, defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
