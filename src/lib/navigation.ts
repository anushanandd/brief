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
  type IconComponent,
  Landmark,
  Palette,
  Percent,
  Plane,
  Receipt,
  ReceiptText,
  ScrollText,
  Settings,
  ShieldCheck,
  Sparkles,
  Sprout,
  Users,
  WalletCards,
} from '../components/icons'
import { analyticsCharts } from './analytics'

export const primaryNavigation = [
  { to: '/', label: 'Home', icon: House, shortcut: '⌘1' },
  { to: '/spending', label: 'Spending', icon: WalletCards, shortcut: '⌘2' },
  { to: '/holdings', label: 'Investments', icon: BriefcaseBusiness, shortcut: '⌘3' },
  { to: '/accounts', label: 'Accounts', icon: Landmark, shortcut: '⌘4' },
  { to: '/activities', label: 'Activity', icon: Activity, shortcut: '⌘5' },
  { to: '/calendar', label: 'Calendar', icon: CalendarDays, shortcut: '⌘6' },
  { to: '/insights', label: 'Insights', icon: Sparkles, shortcut: '⌘7' },
] as const

export type SectionTab = {
  id: 'trips' | 'people' | 'taxes' | 'documents'
  label: string
  to: '/insights' | '/taxes' | '/documents'
  search?: { view: 'people' }
  icon: IconComponent
  color: string
}

/**
 * Pages that live under one sidebar page, shown as the strip of cards that heads each of them,
 * as Spending's reports do; the sidebar page stays highlighted on all of them. Insights holds
 * what Brief works out from your records: trips, the people you pay, and taxes with the
 * documents they read.
 */
export const pageSections: Record<'/insights', SectionTab[]> = {
  '/insights': [
    { id: 'trips', label: 'Trips', to: '/insights', icon: Plane, color: 'var(--data-4)' },
    {
      id: 'people',
      label: 'People',
      to: '/insights',
      search: { view: 'people' },
      icon: Users,
      color: 'var(--data-2)',
    },
    { id: 'taxes', label: 'Taxes', to: '/taxes', icon: ReceiptText, color: 'var(--data-6)' },
    {
      id: 'documents',
      label: 'Documents',
      to: '/documents',
      icon: FileText,
      color: 'var(--data-5)',
    },
  ],
}

/** The sidebar page a path belongs to, when it is one of a page's tabs. */
export function sectionHome(pathname: string): keyof typeof pageSections | undefined {
  return pageSections['/insights'].some(({ to }) => to === pathname) ? '/insights' : undefined
}

/** Which tab a location shows: People is Insights' people view, Trips the rest of Insights. */
export function sectionTabActive(tab: SectionTab, pathname: string, view: unknown) {
  if (tab.to !== pathname) return false
  return tab.to !== '/insights' || (tab.search?.view === 'people') === (view === 'people')
}

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
  'card-credits': CreditCard,
  dividends: Sprout,
  interest: Percent,
  fees: Receipt,
  realized: ChartNoAxesCombined,
}

// Analytics marks share one tone per chart wherever the chart is listed.
export const analyticsIconColors = {
  'cash-flow': 'var(--data-1)',
  income: 'var(--positive)',
  'card-credits': 'var(--positive)',
  dividends: 'var(--positive)',
  interest: 'var(--positive)',
  fees: 'var(--negative)',
  realized: 'var(--chart-benchmark)',
} as const

/** Each destination's own pages, which head its reports in one strip. */
export const destinationPages = {
  money: [{ to: '/spending', label: 'Spending', icon: WalletCards, color: 'var(--negative)' }],
  investments: [
    { to: '/holdings', label: 'Overview', icon: BriefcaseBusiness, color: 'var(--data-1)' },
  ],
} as const

export const analyticsNavigation = analyticsCharts.map((chart) => ({
  to: '/analytics' as const,
  label: chart.label,
  icon: analyticsIcons[chart.id],
  search: { chart: chart.id },
}))
