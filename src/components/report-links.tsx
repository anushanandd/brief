import { useMemo } from 'react'

import { useReports } from '../hooks/use-reports'
import {
  analyticsAccountMatches,
  analyticsCharts,
  analyticsDefinition,
  analyticsGroup,
  benefitYear,
  moneyStripRequests,
  noAnalyticsAccounts,
  reportDay,
  withoutReportView,
  type AnalyticsChart,
  type AnalyticsSearch,
} from '../lib/analytics'
import { financeSettings } from '../lib/finance-settings'
import { analyticsIconColors, analyticsIcons, destinationPages } from '../lib/navigation'
import type { FinanceSnapshot } from '../lib/schema'
import {
  reportRangeForPeriod,
  spendingHeadline,
  spendingPeriodForRange,
  spendingRangeForPeriod,
  type SpendingPeriod,
} from '../lib/spending'
import { SelectorStrip, StripIconMark, type SelectorStripItem } from './selector-strip'

/**
 * Money's strip: Spending, then its reports, each opening on the page's period. Benefits always
 * reads a calendar year, so its card shows that year and leaves the shared period alone.
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
  const range = period == null ? (search.range ?? 'month') : reportRangeForPeriod(period)
  const spendingPeriod = period ?? spendingPeriodForRange(range) ?? 1
  const reportSearch = useMemo(
    () => (period == null ? withoutReportView(search) : { range }),
    [period, range, search],
  )
  // Rust sums every report over the strip's window, through today as the report pages do, and
  // Benefits over its year: the one open on Benefits, else this one.
  const today = reportDay(data)
  const benefitsSearch = useMemo(
    () => ({ account: reportSearch.account, year: search.year }),
    [reportSearch.account, search.year],
  )
  const [summaryRequest, benefitsRequest] = moneyStripRequests(
    { ...reportSearch, ...benefitsSearch },
    today,
  )
  const reports = useReports(data, summaryRequest).data ?? undefined
  const benefitsReports = useReports(data, benefitsRequest).data ?? undefined
  const benefitsYear = benefitYear(benefitsSearch, today)
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
    const {
      icon: SpendingIcon,
      label: spendingLabel,
      color: spendingColor,
    } = destinationPages.money[0]
    return [
      {
        id: 'spending',
        label: spendingLabel,
        mark: (
          <StripIconMark
            icon={<SpendingIcon size={15} />}
            color={spending.hasScope ? spendingColor : 'var(--text-tertiary)'}
          />
        ),
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
          const benefits = analyticsDefinition(id).benefits
          const source = benefits ? benefitsReports : reports
          const report = source?.summaries.find(({ chart }) => chart === id)
          const linkSearch = { ...reportSearch, chart: id, ...(benefits ? benefitsSearch : {}) }
          const unavailable =
            !report ||
            missingAccount ||
            !source?.window.valid ||
            (benefits &&
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
            link: { to: '/analytics', search: linkSearch },
            current: current === id,
            value: report?.total ?? null,
            change: report?.change ?? null,
            favorable: id === 'fees' ? 'decrease' : 'increase',
            unavailable,
            ...(benefits
              ? {
                  detail:
                    benefitsYear === Number(today.slice(0, 4)) ? 'this year' : `in ${benefitsYear}`,
                }
              : {}),
          }
        }),
    ]
  }, [
    benefitsReports,
    benefitsSearch,
    benefitsYear,
    current,
    data,
    reportSearch,
    reports,
    spendingAccount,
    spendingAccountId,
    spendingPeriod,
    today,
  ])
  return <SelectorStrip label="Money" items={items} />
}
