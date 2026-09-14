import {
  createRootRoute,
  createRoute,
  createRouter,
  lazyRouteComponent,
  redirect,
} from '@tanstack/react-router'

import { AppShell } from './components/app-shell'

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
const accountDetailRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/accounts/$accountId',
  component: lazyRouteComponent(() => import('./pages/account-detail'), 'AccountDetailPage'),
})
const ledgerSearch = (
  search: Record<string, unknown>,
): { account?: string; category?: string } => ({
  ...(typeof search.account === 'string' && search.account ? { account: search.account } : {}),
  ...(typeof search.category === 'string' && search.category ? { category: search.category } : {}),
})
const holdingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/holdings',
  validateSearch: ledgerSearch,
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
const spendingActivityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/spending',
  beforeLoad: () => {
    throw redirect({ to: '/spending' })
  },
})
const subscriptionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/spending/subscriptions',
  component: lazyRouteComponent(() => import('./pages/subscriptions'), 'SubscriptionsPage'),
})
const legacySubscriptionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/subscriptions',
  beforeLoad: () => {
    throw redirect({ to: '/spending/subscriptions' })
  },
})
const tradesActivityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/trades',
  component: lazyRouteComponent(() => import('./pages/spending'), 'TradesActivityPage'),
})
const changesActivityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/changes',
  component: lazyRouteComponent(() => import('./pages/spending'), 'ChangesActivityPage'),
})
const transactionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/spending/transactions',
  component: lazyRouteComponent(() => import('./pages/transactions'), 'TransactionsPage'),
})
const legacyTransactionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/transactions',
  beforeLoad: () => {
    throw redirect({ to: '/spending/transactions' })
  },
})
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: lazyRouteComponent(() => import('./pages/settings'), 'SettingsPage'),
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
  accountDetailRoute,
  holdingsRoute,
  activitiesRoute,
  spendingRoute,
  spendingActivityRoute,
  subscriptionsRoute,
  legacySubscriptionsRoute,
  tradesActivityRoute,
  changesActivityRoute,
  transactionsRoute,
  legacyTransactionsRoute,
  settingsRoute,
  logsRoute,
])

export const router = createRouter({ routeTree, defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
