import { useLocation, useNavigate } from '@tanstack/react-router'
import { useTheme } from 'next-themes'
import { useMemo } from 'react'

import { Eye, EyeOff, HelpCircle, List, Palette, ReceiptText, Repeat2 } from '../components/icons'
import { RefreshCw } from '../components/icons'
import type { AppAction } from '../lib/action-system'
import type { AnalyticsChart } from '../lib/analytics'
import { analyticsNavigation, settingsNavigation } from '../lib/navigation'
import { adjacentPage, useOrderedNavigation } from '../lib/navigation-preferences'
import { useSettingsDialog } from '../lib/settings-dialog'
import { useFinance } from './use-finance'
import { useFinanceRefreshState, useRefreshFinance } from './use-refresh-finance'

/** `g` followed by these keys opens the page; pages without one take their sidebar number. */
const navigationSequences: Record<string, string> = {
  '/': 'h',
  '/accounts': 'a',
  '/holdings': 'i',
  '/activities': 'c',
  '/spending': 's',
}
/** `g r` followed by these keys opens the report; `g r t` opens Taxes and `g r u` Recurring. */
const reportSequences: Record<AnalyticsChart, string> = {
  'cash-flow': 'f',
  income: 'i',
  interest: 'n',
  fees: 'e',
  'amex-credits': 'b',
  dividends: 'd',
  realized: 'p',
}

/** What the shell owns that global actions act on. */
export type ShellControls = {
  openPalette: () => void
  openShortcuts: () => void
  openKeyGuide: () => void
  openFocusHints: () => void
  valuesHidden: boolean
  toggleValues: () => void
  toggleReports: () => void
}

/**
 * The actions available everywhere: navigation with its shortcuts, the palette and help
 * overlays, refreshing, privacy, and appearance.
 */
export function useGlobalActions(controls: ShellControls): AppAction[] {
  const pages = useOrderedNavigation()
  const pathname = useLocation({ select: (location) => location.pathname })
  const navigate = useNavigate()
  // Only the callback: the dialog hook's return object is new each render.
  const { show: openSettings } = useSettingsDialog()
  const finance = useFinance()
  const refresh = useRefreshFinance()
  const isRefreshing = useFinanceRefreshState()
  const { theme, setTheme } = useTheme()
  const recovering = Boolean(finance.data?.recovery)
  const plaidConnected = finance.integrationStatus.plaid
  const {
    openPalette,
    openShortcuts,
    openKeyGuide,
    openFocusHints,
    valuesHidden,
    toggleValues,
    toggleReports,
  } = controls
  return useMemo<AppAction[]>(
    () => [
      ...pages.map((page, index) => ({
        id: `navigation.open.${page.to}`,
        title: `Open ${page.label}`,
        search: `go navigate ${page.label}`,
        group: 'Navigate',
        icon: page.icon,
        scope: 'app' as const,
        bindings: [
          { sequence: [`Meta+${index + 1}`], label: `⌘${index + 1}` },
          {
            sequence: ['g', ...(navigationSequences[page.to] ?? String(index + 1)).split(' ')],
            label: `g ${navigationSequences[page.to] ?? String(index + 1)}`,
          },
        ],
        run: () => navigate({ to: page.to }),
      })),
      ...analyticsNavigation.map((report) => ({
        id: `navigation.report.${report.search.chart}`,
        title: `Open ${report.label}`,
        search: `go report ${report.label}`,
        group: 'Navigate',
        icon: report.icon,
        scope: 'app' as const,
        bindings: [
          {
            sequence: ['g', 'r', reportSequences[report.search.chart]],
            label: `g r ${reportSequences[report.search.chart]}`,
          },
        ],
        run: () => navigate({ to: report.to, search: report.search }),
      })),
      {
        id: 'navigation.open./taxes',
        title: 'Open Taxes',
        search: 'go report taxes tax estimate 1099 wash sale harvest capital gains',
        group: 'Navigate',
        icon: ReceiptText,
        scope: 'app' as const,
        bindings: [{ sequence: ['g', 'r', 't'], label: 'g r t' }],
        run: () => navigate({ to: '/taxes' }),
      },
      {
        id: 'navigation.open./recurring',
        title: 'Open Recurring',
        search: 'go report recurring subscriptions bills paydays upcoming price changes',
        group: 'Navigate',
        icon: Repeat2,
        scope: 'app' as const,
        bindings: [{ sequence: ['g', 'r', 'u'], label: 'g r u' }],
        run: () => navigate({ to: '/recurring' }),
      },
      {
        id: 'settings.open',
        title: 'Open Settings',
        search: 'preferences settings options configure',
        group: 'Navigate',
        icon: settingsNavigation.icon,
        scope: 'app',
        bindings: [
          { sequence: ['Meta+,'], label: '⌘,' },
          { sequence: ['g', 's', 'e'], label: 'g s e' },
        ],
        run: () => openSettings(),
      },
      {
        id: 'palette.open',
        title: 'Open command palette',
        search: 'quick actions search commands',
        group: 'Actions',
        icon: List,
        scope: 'app',
        bindings: [{ sequence: ['Meta+k'], label: '⌘K' }],
        run: openPalette,
      },
      {
        id: 'shortcuts.open',
        title: 'Keyboard shortcuts',
        search: 'keyboard shortcuts help reference keys',
        group: 'Actions',
        icon: HelpCircle,
        scope: 'app',
        bindings: [{ sequence: ['Meta+/'], label: '⌘/' }],
        run: openShortcuts,
      },
      {
        id: 'key-guide.open',
        title: 'Show contextual keyboard guide',
        search: 'keyboard shortcuts which key help',
        group: 'Actions',
        icon: HelpCircle,
        scope: 'app',
        bindings: [{ sequence: ['?'], label: '?' }],
        run: openKeyGuide,
      },
      {
        id: 'focus-hints.open',
        title: 'Jump to visible control',
        search: 'focus visible control vim hints',
        group: 'Actions',
        icon: List,
        scope: 'app',
        bindings: [{ sequence: ['g', 'f'], label: 'g f' }],
        run: openFocusHints,
      },
      {
        id: 'finance.refresh',
        title: isRefreshing ? 'Refreshing snapshot…' : 'Refresh snapshot',
        search: 'refresh data finance providers',
        group: 'Actions',
        icon: RefreshCw,
        scope: 'app',
        bindings: [{ sequence: ['Meta+r'], label: '⌘R' }],
        disabled: isRefreshing || recovering,
        disabledReason: recovering
          ? 'Resolve local data recovery first'
          : isRefreshing
            ? 'A refresh is already running'
            : undefined,
        run: () => refresh(),
      },
      {
        id: 'finance.refresh-live',
        title: 'Refresh with live bank balances',
        search: 'refresh live real-time balances plaid billed',
        detail: 'Plaid bills each real-time balance request.',
        group: 'Actions',
        icon: RefreshCw,
        scope: 'app',
        disabled: isRefreshing || recovering || !plaidConnected,
        disabledReason: !plaidConnected
          ? 'Connect Plaid first'
          : isRefreshing
            ? 'A refresh is already running'
            : undefined,
        run: () => refresh({ liveBalances: true }),
      },
      {
        id: 'privacy.toggle-values',
        title: valuesHidden ? 'Show sensitive values' : 'Hide sensitive values',
        search: 'privacy mask hide show balances amounts',
        group: 'Appearance',
        icon: valuesHidden ? Eye : EyeOff,
        scope: 'app',
        bindings: [{ sequence: ['Meta+h'], label: '⌘H' }],
        repeatable: true,
        run: toggleValues,
      },
      {
        id: 'navigation.previous-page',
        title: 'Open previous sidebar page',
        group: 'Navigate',
        scope: 'app',
        bindings: [{ sequence: ['Meta+ArrowUp'], label: '⌘↑' }],
        repeatable: true,
        run: () =>
          navigate({
            to: adjacentPage(
              pages.map(({ to }) => to),
              pathname,
              -1,
            ),
          }),
      },
      {
        id: 'navigation.next-page',
        title: 'Open next sidebar page',
        group: 'Navigate',
        scope: 'app',
        bindings: [{ sequence: ['Meta+ArrowDown'], label: '⌘↓' }],
        repeatable: true,
        run: () =>
          navigate({
            to: adjacentPage(
              pages.map(({ to }) => to),
              pathname,
              1,
            ),
          }),
      },
      {
        id: 'navigation.back',
        title: 'Go back',
        group: 'Navigate',
        scope: 'app',
        bindings: [{ sequence: ['Meta+['], label: '⌘[' }],
        repeatable: true,
        run: () => window.history.back(),
      },
      {
        id: 'navigation.forward',
        title: 'Go forward',
        group: 'Navigate',
        scope: 'app',
        bindings: [{ sequence: ['Meta+]'], label: '⌘]' }],
        repeatable: true,
        run: () => window.history.forward(),
      },
      {
        id: 'navigation.reports.toggle',
        title: 'Show or hide reports in the sidebar',
        search: 'sidebar reports expand collapse',
        group: 'View',
        scope: 'app',
        run: toggleReports,
      },
      ...(['dark', 'custom'] as const).map((nextTheme) => ({
        id: `appearance.theme.${nextTheme}`,
        title: `Switch to ${nextTheme[0].toUpperCase()}${nextTheme.slice(1)} theme`,
        search: `${nextTheme} mode appearance theme`,
        group: 'Appearance',
        icon: Palette,
        scope: 'app' as const,
        disabled: theme === nextTheme,
        disabledReason: theme === nextTheme ? `${nextTheme} theme is already active` : undefined,
        run: () => setTheme(nextTheme),
      })),
    ],
    [
      isRefreshing,
      navigate,
      openFocusHints,
      openKeyGuide,
      openPalette,
      openShortcuts,
      pages,
      pathname,
      plaidConnected,
      recovering,
      refresh,
      openSettings,
      setTheme,
      theme,
      toggleReports,
      toggleValues,
      valuesHidden,
    ],
  )
}
