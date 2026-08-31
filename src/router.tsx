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
const analyticsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/analytics',
  component: lazyRouteComponent(() => import('./pages/analytics'), 'AnalyticsPage'),
})
const holdingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/holdings',
  component: lazyRouteComponent(() => import('./pages/holdings'), 'HoldingsPage'),
})
const spendingRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/spending',
  component: lazyRouteComponent(() => import('./pages/spending'), 'SpendingPage'),
})
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings',
  component: lazyRouteComponent(() => import('./pages/settings'), 'SettingsPage'),
})

const routeTree = rootRoute.addChildren([
  dashboardRoute,
  analyticsRoute,
  holdingsRoute,
  spendingRoute,
  settingsRoute,
])

export const router = createRouter({ routeTree, defaultPreload: 'intent' })

declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router
  }
}
