import {
  createRootRoute,
  createRoute,
  createRouter,
  lazyRouteComponent,
} from '@tanstack/react-router'

import { AppShell } from './components/app-shell'

const rootRoute = createRootRoute({ component: AppShell })

const dashboardRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  component: lazyRouteComponent(() => import('./pages/dashboard'), 'DashboardPage'),
})
const accountsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/accounts',
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
const activitiesRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities',
  component: lazyRouteComponent(() => import('./pages/spending'), 'ActivitiesPage'),
})
const spendingActivityRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/spending',
  component: lazyRouteComponent(() => import('./pages/spending'), 'SpendingActivityPage'),
})
const subscriptionsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/subscriptions',
  component: lazyRouteComponent(() => import('./pages/subscriptions'), 'SubscriptionsPage'),
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
  path: '/activities/transactions',
  component: lazyRouteComponent(() => import('./pages/transactions'), 'TransactionsPage'),
})
const platinumBenefitsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/activities/benefits',
  component: lazyRouteComponent(() => import('./pages/platinum-benefits'), 'PlatinumBenefitsPage'),
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
  activitiesRoute,
  spendingActivityRoute,
  subscriptionsRoute,
  tradesActivityRoute,
  changesActivityRoute,
  transactionsRoute,
  platinumBenefitsRoute,
  settingsRoute,
  logsRoute,
])

export const router = createRouter({ routeTree, defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
