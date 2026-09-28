import {
  Activity,
  ArrowDownUp,
  BadgeDollarSign,
  ChartNoAxesCombined,
  BriefcaseBusiness,
  CreditCard,
  House,
  Landmark,
  Palette,
  Percent,
  Receipt,
  ScrollText,
  Settings,
  ShieldCheck,
  Sprout,
  WalletCards,
} from '../components/icons'
import { analyticsCharts } from './analytics'

export const primaryNavigation = [
  { to: '/', label: 'Home', icon: House, shortcut: '1' },
  { to: '/accounts', label: 'Accounts', icon: Landmark, shortcut: '2' },
  { to: '/spending', label: 'Spending', icon: WalletCards, shortcut: '3' },
  { to: '/analytics', label: 'Analytics', icon: ChartNoAxesCombined, shortcut: '4' },
  { to: '/holdings', label: 'Holdings', icon: BriefcaseBusiness, shortcut: '5' },
  { to: '/activities', label: 'Activity', icon: Activity, shortcut: '6' },
] as const

export const settingsNavigation = {
  to: '/settings',
  label: 'Settings',
  icon: Settings,
  shortcut: '7',
} as const

export const navigation = [...primaryNavigation, settingsNavigation] as const

export const secondaryNavigation = [
  { to: '/health', label: 'Data health', icon: ShieldCheck },
  { to: '/logs', label: 'Diagnostics', icon: ScrollText },
  { to: '/settings/design', label: 'Design', icon: Palette },
] as const

const analyticsIcons = {
  'cash-flow': ArrowDownUp,
  income: BadgeDollarSign,
  'amex-credits': CreditCard,
  dividends: Sprout,
  interest: Percent,
  fees: Receipt,
  realized: ChartNoAxesCombined,
}

export const analyticsNavigation = analyticsCharts.map((chart) => ({
  to: '/analytics' as const,
  label: chart.label,
  icon: analyticsIcons[chart.id],
  search: { chart: chart.id },
}))

const navigationShortcutRange = `${navigation[0].shortcut}–${navigation.at(-1)?.shortcut}`

export const shortcutGroups = [
  {
    title: 'Navigation',
    shortcuts: [
      [navigationShortcutRange, 'Open primary pages'],
      ['?', 'Show keyboard shortcuts'],
      ['⌘K', 'Search actions and activity'],
      ['? in quick actions', 'Ask Brief'],
      ['⌘[ / ⌘]', 'Go back / forward'],
      ['⌘↑ / ⌘↓', 'Previous / next sidebar page'],
      ['⌘,', 'Open Settings'],
      ['⌘R', 'Refresh data'],
      ['⌘H', 'Hide / show sensitive values'],
    ],
  },
  {
    title: 'Lists and search',
    shortcuts: [
      ['/', 'Focus search'],
      ['esc', 'Clear search, then blur'],
      ['J / K', 'Move down / up in the focused list'],
      ['return', 'Open the focused row'],
    ],
  },
  {
    title: 'Activity',
    shortcuts: [
      ['F', 'Open filters'],
      ['C', 'Clear all filters'],
      ['↑ / ↓ · return', 'Choose an account, category, or method'],
      ['tab', 'Adjust the date range'],
      ['tab · return', 'Remove a selected filter tag'],
    ],
  },
  {
    title: 'Spending',
    shortcuts: [
      ['S / W / M / Q / Y / A', 'Statement / week / month / quarter / year / all time'],
      ['← / →', 'Previous / next period'],
    ],
  },
  {
    title: 'Charts',
    shortcuts: [
      ['W / M / Q / Y / A', 'Week / month / quarter / year / all time'],
      ['D / W / M / Y / A', 'Holdings: day / week / month / year / all time'],
      ['← / →', 'Previous / next account, analytics chart, or holding'],
      ['⌘⌃← / ⌘⌃→', 'Previous / next date view'],
    ],
  },
] as const
