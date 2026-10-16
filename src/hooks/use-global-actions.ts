import { useLocation, useNavigate } from '@tanstack/react-router'
import { useTheme } from 'next-themes'
import { useMemo } from 'react'
import { toast } from 'sonner'

import {
  CalendarDays,
  Download,
  Eye,
  EyeOff,
  HelpCircle,
  List,
  MapPinned,
  Palette,
  Plane,
  ReceiptText,
  Repeat2,
  RotateCcw,
  Sunset,
  Users,
} from '../components/icons'
import { RefreshCw } from '../components/icons'
import type { AppAction } from '../lib/action-system'
import type { AnalyticsChart } from '../lib/analytics'
import { errorMessage } from '../lib/errors'
import { exportLotsCsv, exportUpcomingCalendar } from '../lib/exports'
import { analyticsNavigation, settingsNavigation } from '../lib/navigation'
import { adjacentPage, useOrderedNavigation } from '../lib/navigation-preferences'
import { useSettingsDialog } from '../lib/settings-dialog'
import { useFinance } from './use-finance'
import { useFinanceEdits } from './use-finance-edits'
import { useFindPlaces } from './use-find-places'
import { useFinanceRefreshState, useRefreshFinance } from './use-refresh-finance'

/** `g` followed by these keys opens the page; pages without one take their sidebar number. */
const navigationSequences: Record<string, string> = {
  '/': 'h',
  '/accounts': 'a',
  '/holdings': 'i',
  '/activities': 'c',
  '/spending': 's',
  '/insights': 'n',
  '/calendar': 'd',
  '/documents': 'o',
}
/**
 * `g r` followed by these keys opens the report; `g r t` opens Taxes, `g r u` Recurring,
 * `g r a` Trips (away), and `g r r` Retirement.
 */
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
  const { undo, undoLabel } = useFinanceEdits()
  const recovering = Boolean(finance.data?.recovery)
  const plaidConnected = finance.integrationStatus.plaid
  const data = finance.data
  const hasLots = Boolean(data?.insights?.lots.some((position) => position.lots.length))
  const {
    running: findingPlaces,
    done: placesDone,
    total: placesTotal,
    start: findPlaces,
  } = useFindPlaces()
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
        id: 'navigation.open./holdings/retirement',
        title: 'Open Retirement',
        search: 'go retirement 401k ira roth contributions limit savings rate fire',
        group: 'Navigate',
        icon: Sunset,
        scope: 'app' as const,
        bindings: [{ sequence: ['g', 'r', 'r'], label: 'g r r' }],
        run: () => navigate({ to: '/holdings/retirement' }),
      },
      {
        // Subscriptions live on Spending now; the old Recurring keys still lead to them.
        id: 'navigation.open.subscriptions',
        title: 'Open Subscriptions',
        search: 'go recurring subscriptions monthly price changes',
        group: 'Navigate',
        icon: Repeat2,
        scope: 'app' as const,
        bindings: [{ sequence: ['g', 'r', 'u'], label: 'g r u' }],
        run: () => navigate({ to: '/spending' }),
      },
      {
        id: 'navigation.open./trips',
        title: 'Open Trips',
        search: 'go trips travel away places calendar',
        group: 'Navigate',
        icon: Plane,
        scope: 'app' as const,
        bindings: [{ sequence: ['g', 'r', 'a'], label: 'g r a' }],
        run: () => navigate({ to: '/insights' }),
      },
      {
        id: 'navigation.open.people',
        title: 'Open People',
        search: 'go people person friends zelle venmo contacts',
        group: 'Navigate',
        icon: Users,
        scope: 'app' as const,
        bindings: [{ sequence: ['g', 'p'], label: 'g p' }],
        run: () => navigate({ to: '/insights', search: { view: 'people' } }),
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
      {
        id: 'edits.undo',
        title: undoLabel ? `Undo ${undoLabel}` : 'Undo last edit',
        search: 'undo revert edit annotation category tag note',
        group: 'Actions',
        icon: RotateCcw,
        scope: 'app',
        bindings: [{ sequence: ['Meta+z'], label: '⌘Z' }],
        disabled: !undoLabel,
        disabledReason: 'Nothing to undo in this session',
        run: () =>
          undo()
            .then((undone) => {
              if (undone) toast.success(`Undid ${undone.label}`)
            })
            .catch((error: unknown) =>
              toast.error('Could not undo', { description: errorMessage(error) }),
            ),
      },
      {
        id: 'finance.export-calendar',
        title: 'Export upcoming dates as a calendar',
        search: 'ics calendar bills due statement close dividends export',
        group: 'Actions',
        icon: CalendarDays,
        scope: 'app',
        disabled: !data?.insights,
        disabledReason: 'Load a snapshot first',
        run: () =>
          data &&
          exportUpcomingCalendar(data).catch((error: unknown) =>
            toast.error('Could not save the calendar', { description: errorMessage(error) }),
          ),
      },
      {
        id: 'places.find',
        title: findingPlaces
          ? `Finding places… ${placesDone} of ${placesTotal || '…'} cities`
          : 'Find places',
        search: 'places businesses map match stores overture where',
        detail: 'Matches purchases made in person to their business with Overture Maps.',
        group: 'Actions',
        icon: MapPinned,
        scope: 'app',
        disabled: findingPlaces || recovering || !data?.places?.unsearched,
        disabledReason: findingPlaces
          ? 'Brief is already finding places'
          : recovering
            ? 'Resolve local data recovery first'
            : 'Every place is already looked up',
        run: () =>
          void findPlaces().catch((error: unknown) =>
            toast.error('Could not find places', { description: errorMessage(error) }),
          ),
      },
      {
        id: 'investments.export-lots',
        title: 'Export tax lots as CSV',
        search: 'lots csv tax accountant cost basis export download',
        group: 'Actions',
        icon: Download,
        scope: 'app',
        disabled: !hasLots,
        disabledReason: 'No open tax lots to export',
        run: () =>
          data &&
          exportLotsCsv(data).catch((error: unknown) =>
            toast.error('Could not save the lots', { description: errorMessage(error) }),
          ),
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
      data,
      hasLots,
      isRefreshing,
      navigate,
      undo,
      undoLabel,
      openFocusHints,
      openKeyGuide,
      openPalette,
      openShortcuts,
      pages,
      pathname,
      findingPlaces,
      findPlaces,
      placesDone,
      placesTotal,
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
