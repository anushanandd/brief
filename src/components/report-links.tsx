import { useMemo } from 'react'

import { useReports } from '../hooks/use-reports'
import {
  analyticsAccountMatches,
  analyticsCharts,
  analyticsDefinition,
  analyticsGroup,
  noAnalyticsAccounts,
  reportRequest,
  withoutReportView,
  type AnalyticsChart,
  type AnalyticsSearch,
} from '../lib/analytics'
import { financeSettings } from '../lib/finance-settings'
import { type ChartPeriod } from '../lib/graph-preferences'
import { analyticsIconColors, analyticsIcons, destinationPages } from '../lib/navigation'
import type { FinanceSnapshot } from '../lib/schema'
import {
  spendingHeadline,
  spendingPeriodForRange,
  spendingRangeForPeriod,
  type SpendingPeriod,
} from '../lib/spending'
import { SelectorStrip, StripIconMark, type SelectorStripItem } from './selector-strip'

// Reports have no statement period; a statement opens them on the month.
const reportRange = (period: SpendingPeriod): ChartPeriod => {
  const range = spendingRangeForPeriod(period)
  return range === 'statement' ? 'month' : range
}

/**
 * Money's strip: Spending, then its reports. Every card reports the page's period and opens on
 * it, so moving between them keeps both the frame and the period.
 */
export function MoneyStrip({
  data,
  current,
  period,
  search = {},
  spendingAccount,
}: {
  data: FinanceSnapshot
  current: '/spending' | AnalyticsChart
  /** Spending's period, on Spending. */
  period?: SpendingPeriod
  /** The report's search, on a report. */
  search?: AnalyticsSearch
  /** Spending's account scope, on Spending. */
  spendingAccount?: string
}) {
  const settings = financeSettings(data)
  const spendingAccountId = settings.spendingAccountId ?? ''
  const range = period == null ? (search.range ?? 'month') : reportRange(period)
  const spendingPeriod = period ?? spendingPeriodForRange(range) ?? 1
  const reportSearch = useMemo(
    () => (period == null ? withoutReportView(search) : { range }),
    [period, range, search],
  )
  // Rust sums every report over the strip's window, through today as the report pages do.
  const reports = useReports(data, reportRequest(reportSearch)).data ?? undefined
  const items = useMemo<SelectorStripItem[]>(() => {
    const missingAccount = Boolean(
      reportSearch.account &&
      reportSearch.account !== noAnalyticsAccounts &&
      !data.accounts.some(
        ({ id, currency }) =>
          id !== 'all' &&
          (!currency || currency === 'USD') &&
          analyticsAccountMatches(reportSearch.account, id),
      ),
    )
    const spending = spendingHeadline(data, spendingAccountId, spendingPeriod, spendingAccount)
    const SpendingIcon = destinationPages.money[0].icon
    return [
      {
        id: 'spending',
        label: destinationPages.money[0].label,
        mark: <StripIconMark icon={<SpendingIcon size={15} />} color="var(--text-secondary)" />,
        link: {
          to: '/spending',
          search: { account: spendingAccount, range: spendingRangeForPeriod(spendingPeriod) },
        },
        current: current === '/spending',
        value: spending.value,
        change: spending.change,
        favorable: 'decrease',
        unavailable: !spending.hasScope,
      },
      ...analyticsCharts
        .filter(({ id }) => analyticsGroup(id) === 'money')
        .map(({ id, label }): SelectorStripItem => {
          const Icon = analyticsIcons[id]
          const report = reports?.summaries.find(({ chart }) => chart === id)
          const unavailable =
            !report ||
            missingAccount ||
            !reports?.window.valid ||
            (analyticsDefinition(id).benefits &&
              !data.accounts.some(
                ({ id: accountId, type }) => accountId === spendingAccountId && type === 'credit',
              ))
          return {
            id,
            label,
            mark: (
              <StripIconMark
                icon={<Icon size={15} />}
                color={unavailable ? 'var(--text-tertiary)' : analyticsIconColors[id]}
              />
            ),
            link: { to: '/analytics', search: { ...reportSearch, chart: id } },
            current: current === id,
            value: report?.total ?? null,
            change: report?.change ?? null,
            favorable: id === 'fees' ? 'decrease' : 'increase',
            unavailable,
          }
        }),
    ]
  }, [current, data, reportSearch, reports, spendingAccount, spendingAccountId, spendingPeriod])
  return <SelectorStrip label="Money" items={items} />
}
