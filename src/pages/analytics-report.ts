import { useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useEffect, useMemo, useState } from 'react'

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
import { localDateKey } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import {
  chartPeriodSeconds,
  graphWindows,
  useChosenGraphWindow,
  useGraphWindow,
  type ChartPeriod,
} from '../lib/graph-preferences'
import { getExternalLogosEnabled } from '../lib/logos'
import type { FinanceSnapshot } from '../lib/schema'
import { resolveSpendingAccount } from '../lib/spending'

// One fixed order everywhere a chart is listed.
const orderedCharts = analyticsCharts.map(({ id }) => id)

/** Today's local date, kept current across midnight and whenever the window regains focus. */
function useLocalDay() {
  const [day, setDay] = useState(() => localDateKey(Date.now() / 1000))
  useEffect(() => {
    const update = () => setDay(localDateKey(Date.now() / 1000))
    const timer = globalThis.setInterval(update, 60_000)
    globalThis.addEventListener('focus', update)
    return () => {
      globalThis.clearInterval(timer)
      globalThis.removeEventListener('focus', update)
    }
  }, [])
  return day
}

/**
 * The route's search with its range resolved: the URL's, else the range chosen on any page
 * this session, else the report's default. Choosing a range, by control or shortcut, shares it
 * with the other pages and clears custom dates.
 */
function useAnalyticsSearch() {
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
  useGraphWindowShortcuts(
    useCallback(
      (next) => {
        const current = chartPeriodSeconds(search.range ?? 'month')
        const seconds = typeof next === 'function' ? next(current) : next
        const range = graphWindows.find(({ secs }) => secs === seconds)?.name
        if (range) setRange(range)
      },
      [search.range, setRange],
    ),
  )
  return { search, navigate, setRange }
}

/**
 * The report the page shows and everything it reads: the search, the window and accounts
 * Rust read it over, why it might be unavailable, and the activity and account rows beside it.
 */
export function useAnalyticsReport(data: FinanceSnapshot) {
  // Reports end on the day the insights were computed for, the same day Spending and Home
  // use, so one strip never shows two months. The clock takes over only without insights.
  const clock = useLocalDay()
  const today = data.insights?.today ?? clock
  const { search, navigate, setRange } = useAnalyticsSearch()
  const selected = search.chart ?? analyticsCharts[0].id
  const definition = analyticsDefinition(selected)
  // Benefits runs on one calendar year of its own; the strip keeps the period it shares.
  const benefitsYear = definition.benefits ? benefitYear(search, today) : undefined
  const request = pageReportRequest(search, selected, today)
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
