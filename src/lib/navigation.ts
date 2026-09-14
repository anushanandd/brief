import {
  Activity,
  BriefcaseBusiness,
  House,
  Landmark,
  RefreshCw,
  Settings,
  WalletCards,
} from 'lucide-react'

export const primaryNavigation = [
  { to: '/', label: 'Home', icon: House, shortcut: '1' },
  { to: '/accounts', label: 'Accounts', icon: Landmark, shortcut: '2' },
  { to: '/spending', label: 'Spending', icon: WalletCards, shortcut: '3' },
  { to: '/holdings', label: 'Holdings', icon: BriefcaseBusiness, shortcut: '4' },
  { to: '/activities', label: 'Activity', icon: Activity, shortcut: '5' },
] as const

export const settingsNavigation = {
  to: '/settings',
  label: 'Settings',
  icon: Settings,
  shortcut: '6',
} as const

export const navigation = [...primaryNavigation, settingsNavigation] as const

export const commandDestinations = [
  ...navigation,
  { label: 'Investment accounts', to: '/accounts/investments', icon: Landmark },
  { label: 'Cash accounts', to: '/accounts/cash', icon: Landmark },
  { label: 'Subscriptions', to: '/spending/subscriptions', icon: RefreshCw },
] as const

const navigationShortcutRange = `${navigation[0].shortcut}–${navigation.at(-1)?.shortcut}`

export const shortcutGroups = [
  {
    title: 'Navigation',
    shortcuts: [
      [navigationShortcutRange, 'Open primary pages'],
      ['?', 'Show keyboard shortcuts'],
      ['⌘K', 'Open quick actions'],
      ['⌘[ / ⌘]', 'Go back / forward'],
      ['⌘,', 'Open Settings'],
      ['⌘R', 'Refresh data'],
    ],
  },
  {
    title: 'Lists and search',
    shortcuts: [
      ['/', 'Focus search'],
      ['esc', 'Clear search, then blur'],
      ['J / K', 'Move down / up'],
      ['return', 'Open the focused row'],
      ['← / →', 'Previous / next Activity page'],
    ],
  },
  {
    title: 'Spending',
    shortcuts: [
      ['M / Q / Y / A', 'Month / quarter / year / all time'],
      ['← / →', 'Previous / next month'],
    ],
  },
  {
    title: 'Charts',
    shortcuts: [
      ['W / M / Q / A', 'Week / month / quarter / all time'],
      ['⌘← / ⌘→', 'Previous / next account graph'],
      ['⌘⌃← / ⌘⌃→', 'Previous / next date view'],
    ],
  },
] as const
