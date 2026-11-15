import { useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useMemo, type Dispatch, type SetStateAction } from 'react'

import { useAmountsHidden } from '../hooks/use-amounts-hidden'
import { useGraphWindowShortcuts } from '../hooks/use-graph-window-shortcuts'
import { useReports } from '../hooks/use-reports'
import { accountDisplayName } from '../lib/account-names'
import { buildActivities } from '../lib/activity'
import {
  analyticsAccountMatches,
  analyticsCharts,
  analyticsDefinition,
  analyticsGroup,
  analyticsGroupColors,
  benefitYear,
  emptyReport,
  noAnalyticsAccounts,
  pageReportRequest,
  resolvedAnalyticsSearch,
} from '../lib/analytics'
import { snapshotToday } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import {
  chartPeriodSeconds,
  graphWindows,
  useChosenGraphWindow,
  useGraphEnd,
  useGraphWindow,
  type ChartPeriod,
} from '../lib/graph-preferences'
import { getExternalLogosEnabled } from '../lib/logos'
import type { FinanceSnapshot } from '../lib/schema'
import { resolveSpendingAccount } from '../lib/spending'

// One fixed order everywhere a chart is listed.
const orderedCharts = analyticsCharts.map(({ id }) => id)

/**
 * The route's search with its range resolved: the URL's, else the range chosen on any page
 * this session, else the report's default. Choosing a range, by control or shortcut, shares it
 * with the other pages and clears custom dates.
 */
function useAnalyticsSearch(data: FinanceSnapshot, today: string) {
  const routeSearch = useSearch({ from: '/analytics' })
  const navigate = useNavigate({ from: '/analytics' })
  const [, setSharedWindow] = useGraphWindow()
  const chosenWindow = useChosenGraphWindow()
  const search = useMemo(
    () => resolvedAnalyticsSearch(routeSearch, chosenWindow),
    [routeSearch, chosenWindow],
  )
  const setRange = useCallback(
    (range: ChartPeriod) => {
      setSharedWindow(chartPeriodSeconds(range))
      void navigate({ search: { ...search, range, from: undefined, to: undefined } })
    },
    [navigate, search, setSharedWindow],
  )
  const setWindow = useCallback<Dispatch<SetStateAction<number>>>(
    (next) => {
      const current = chartPeriodSeconds(search.range ?? 'month')
      const seconds = typeof next === 'function' ? next(current) : next
      const range = graphWindows.find(({ secs }) => secs === seconds)?.name
      if (range) setRange(range)
    },
    [search.range, setRange],
  )
  // A trailing period scrolls back through the saved activity; custom dates and Benefits'
  // calendar year don't.
  const first = useMemo(
    () =>
      data.transactions.reduce<string | undefined>((earliest, transaction) => {
        // Reports date a purchase by the day it was made (decision 0029).
        const date = transaction.date.slice(0, 10)
        return !earliest || date < earliest ? date : earliest
      }, undefined),
    [data.transactions],
  )
  const fixed =
    Boolean(search.from || search.to) ||
    analyticsDefinition(search.chart ?? analyticsCharts[0].id).benefits
  const scroll = useMemo(() => (fixed ? undefined : { today, first }), [fixed, today, first])
  useGraphWindowShortcuts(setWindow, scroll)
  return { search, navigate, setRange, setWindow, scroll }
}

/**
 * The report the page shows and everything it reads: the search, the window and accounts
 * Rust read it over, why it might be unavailable, and the activity and account rows beside it.
 */
export function useAnalyticsReport(data: FinanceSnapshot) {
  // Reports end on the day the insights were computed for, the same day Spending and Home
  // use, so one strip never shows two months.
  const today = snapshotToday(data)
  const { search, navigate, setRange, setWindow, scroll } = useAnalyticsSearch(data, today)
  const [graphEnd] = useGraphEnd()
  const selected = search.chart ?? analyticsCharts[0].id
  const definition = analyticsDefinition(selected)
  // Benefits runs on one calendar year of its own; the strip keeps the period it shares.
  const benefitsYear = definition.benefits ? benefitYear(search, today) : undefined
  // Scrolled back in time, the period ends on the page's end day.
  const end = scroll ? graphEnd : undefined
  const request = {
    ...pageReportRequest(search, selected, today),
    ...(end ? { to: end } : {}),
  }
  const settings = financeSettings(data)
  const spendingAccountId = settings.spendingAccountId ?? ''
  const spendingAccount = resolveSpendingAccount(data.accounts, spendingAccountId)
  const names = settings.accountNames
  const externalLogosEnabled = getExternalLogosEnabled()
  // Rust reads every report over the window and accounts, and this one in full.
  const reportsQuery = useReports(data, request)
  const reports = reportsQuery.data ?? undefined
  const reportWindow = reports?.window ?? {
    start: today,
    end: today,
    range: request.range ?? 'month',
    custom: Boolean(request.from || request.to),
    valid: true,
  }
  const report = { label: definition.label, ...(reports?.report ?? emptyReport(selected)) }
  // Reports sit under Money or Investments; the switcher and arrows stay within one.
  const group = analyticsGroup(selected)
  const groupReports = orderedCharts.filter((id) => analyticsGroup(id) === group)
  const accounts = data.accounts.filter(
    ({ id, currency }) => id !== 'all' && (!currency || currency === 'USD'),
  )
  const missingAccount = Boolean(
    search.account &&
    search.account !== noAnalyticsAccounts &&
    !accounts.some(({ id }) => analyticsAccountMatches(search.account, id)),
  )
  const needsSpendingAccount = definition.benefits && !spendingAccount
  const incompatibleBenefitAccount =
    definition.benefits &&
    search.account !== undefined &&
    !analyticsAccountMatches(search.account, spendingAccountId)
  // Without reports (a window the preview has none for), nothing reads as a figure.
  const unavailable =
    needsSpendingAccount ||
    missingAccount ||
    incompatibleBenefitAccount ||
    !reportWindow.valid ||
    !reports
  const valuesHidden = useAmountsHidden()
  // Activities carry formatted amounts, which screen privacy masks.
  const activities = useMemo(
    () =>
      new Map(buildActivities(data, names, externalLogosEnabled).map((item) => [item.id, item])),
    [data, names, externalLogosEnabled, valuesHidden],
  )
  const groupColors = useMemo(() => analyticsGroupColors(reports?.groups ?? []), [reports])
  const accountTotals = new Map(reports?.accounts.map(({ accountId, total }) => [accountId, total]))
  const accountRows = accounts.flatMap(({ id, name, type, institution, domain }) =>
    accountTotals.has(id)
      ? [
          {
            id,
            type,
            institution,
            domain,
            label: accountDisplayName(id, name, names),
            value: accountTotals.get(id) ?? null,
          },
        ]
      : [],
  )
  const expectedRecurringIds = useMemo(() => new Set(reports?.recurringIds), [reports])
  return {
    today,
    search,
    navigate,
    setRange,
    setWindow,
    scroll,
    /** Whether the report is the trailing period, though Rust reads an end day as custom. */
    custom: reportWindow.custom && !end,
    selected,
    definition,
    benefitsYear,
    spendingAccount,
    reportsQuery,
    reportWindow,
    report,
    group,
    groupReports,
    missingAccount,
    needsSpendingAccount,
    incompatibleBenefitAccount,
    unavailable,
    activities,
    groupColors,
    accountRows,
    expectedRecurringIds,
    /** Activity's search for the report's entries over its window. */
    activitySearch: {
      analysis: selected,
      account: search.account,
      from: reportWindow.start,
      to: reportWindow.end,
    },
    flow: definition.flow ? (reports?.flow ?? null) : null,
  }
}
