import {
  createRootRoute,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
} from '@tanstack/react-router'

import { AppShell } from './components/app-shell'
import { analyticsChart, analyticsSearch, validDate, type AnalyticsChart } from './lib/analytics'

const rootRoute = createRootRoute({ component: AppShell })

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: lazyRouteComponent(() => import('./pages/dashboard'), 'DashboardPage'),
})
const accountSearch = (search: Record<string, unknown>): { account?: string } =>
  typeof search.account === 'string' && search.account ? { account: search.account } : {}
const accountsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/accounts',
  validateSearch: accountSearch,
  component: lazyRouteComponent(() => import('./pages/accounts'), 'AccountsPage'),
})
const investmentAccountsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/accounts/investments',
  component: lazyRouteComponent(() => import('./pages/accounts'), 'InvestmentAccountsPage'),
})
const cashAccountsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/accounts/cash',
  component: lazyRouteComponent(() => import('./pages/accounts'), 'CashAccountsPage'),
})
const ledgerSearch = (
  search: Record<string, unknown>,
): {
  account?: string
  category?: string
  from?: string
  to?: string
  analysis?: AnalyticsChart
} => {
  const from = validDate(search.from)
  const to = validDate(search.to)
  return {
    ...(analyticsChart(search.analysis) ? { analysis: analyticsChart(search.analysis) } : {}),
    ...(typeof search.account === 'string' && search.account ? { account: search.account } : {}),
    ...(typeof search.category === 'string' && search.category
      ? { category: search.category }
      : {}),
    ...(from ? { from } : {}),
    ...(to ? { to } : {}),
  }
}
const holdingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/holdings',
  validateSearch: (search: Record<string, unknown>): { ticker?: string } =>
    typeof search.ticker === 'string' && search.ticker ? { ticker: search.ticker } : {},
  component: lazyRouteComponent(() => import('./pages/holdings'), 'HoldingsPage'),
})
const activitiesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities',
  validateSearch: ledgerSearch,
  component: lazyRouteComponent(() => import('./pages/activities'), 'ActivitiesPage'),
})
const spendingRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/spending',
  component: lazyRouteComponent(() => import('./pages/spending'), 'SpendingPage'),
})
const platinumBenefitsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/spending/platinum',
  component: lazyRouteComponent(() => import('./pages/platinum-benefits'), 'PlatinumBenefitsPage'),
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
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: lazyRouteComponent(() => import('./pages/settings'), 'SettingsPage'),
})
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

const routeTree = rootRoute.addChildren([
  dashboardRoute,
  accountsRoute,
  investmentAccountsRoute,
  cashAccountsRoute,
  holdingsRoute,
  activitiesRoute,
  spendingRoute,
  platinumBenefitsRoute,
  moneyRoute,
  analyticsRoute,
  spendingActivityRoute,
  settingsRoute,
  designRoute,
  logsRoute,
])

export const router = createRouter({ routeTree, defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
