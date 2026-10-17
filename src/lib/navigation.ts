import {
  Activity,
  ArrowDownUp,
  BadgeDollarSign,
  ChartNoAxesCombined,
  BriefcaseBusiness,
  CalendarDays,
  CreditCard,
  FileText,
  House,
  Landmark,
  Palette,
  Percent,
  Receipt,
  ReceiptText,
  ScrollText,
  Settings,
  ShieldCheck,
  Sparkles,
  Sprout,
  WalletCards,
} from '../components/icons'
import { analyticsCharts } from './analytics'

export const primaryNavigation = [
  { to: '/', label: 'Home', icon: House, shortcut: '⌘1' },
  { to: '/spending', label: 'Money', icon: WalletCards, shortcut: '⌘2' },
  { to: '/holdings', label: 'Investments', icon: BriefcaseBusiness, shortcut: '⌘3' },
  { to: '/accounts', label: 'Accounts', icon: Landmark, shortcut: '⌘4' },
  { to: '/activities', label: 'Activity', icon: Activity, shortcut: '⌘5' },
  { to: '/insights', label: 'Insights', icon: Sparkles, shortcut: '⌘6' },
  { to: '/calendar', label: 'Calendar', icon: CalendarDays, shortcut: '⌘7' },
  { to: '/documents', label: 'Documents', icon: FileText, shortcut: '⌘8' },
  { to: '/taxes', label: 'Taxes', icon: ReceiptText, shortcut: '⌘9' },
] as const

// Settings opens as a dialog over the current page rather than a sidebar page.
export const settingsNavigation = { label: 'Settings', icon: Settings, shortcut: '⌘,' } as const

export const navigation = primaryNavigation

export const secondaryNavigation = [
  { to: '/health', label: 'Data health', icon: ShieldCheck },
  { to: '/logs', label: 'Diagnostics', icon: ScrollText },
  ...(import.meta.env.DEV
    ? ([{ to: '/settings/design', label: 'Design', icon: Palette }] as const)
    : []),
] as const

export const analyticsIcons = {
  'cash-flow': ArrowDownUp,
  income: BadgeDollarSign,
  'amex-credits': CreditCard,
  dividends: Sprout,
  interest: Percent,
  fees: Receipt,
  realized: ChartNoAxesCombined,
}

// Analytics marks share one tone per chart wherever the chart is listed.
export const analyticsIconColors = {
  'cash-flow': 'var(--chart-secondary)',
  income: 'var(--positive)',
  'amex-credits': 'var(--positive)',
  dividends: 'var(--positive)',
  interest: 'var(--positive)',
  fees: 'var(--negative)',
  realized: 'var(--chart-benchmark)',
} as const

/** Each destination's own pages, which head its reports in one strip. */
export const destinationPages = {
  money: [{ to: '/spending', label: 'Spending', icon: WalletCards }],
  investments: [{ to: '/holdings', label: 'Overview', icon: BriefcaseBusiness }],
} as const

export const analyticsNavigation = analyticsCharts.map((chart) => ({
  to: '/analytics' as const,
  label: chart.label,
  icon: analyticsIcons[chart.id],
  search: { chart: chart.id },
}))
