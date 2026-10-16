import { Link } from '@tanstack/react-router'
import type { ReactElement } from 'react'

import type { accountTransactionSummary } from '../lib/account-summary'
import { formatCurrency, formatShare, valueTone, formatShortDay } from '../lib/format'
import { type ChartPeriod } from '../lib/graph-preferences'
import type { Account, Insights } from '../lib/schema'
import type { buildSpendingView } from '../lib/spending'
import { Metric } from './ui'

type TransactionSummary = ReturnType<typeof accountTransactionSummary>

const aprLabels: Record<string, string> = {
  balance_transfer_apr: 'transfers',
  cash_apr: 'cash',
  special: 'promo',
}

/** A report's total for the account, linking to the report over the same range. */
export function ReportMetric({
  chart,
  label,
  value,
  range,
  accountId,
}: {
  chart: 'interest' | 'dividends'
  label: string
  value: number
  range: ChartPeriod
  accountId?: string
}) {
  return (
    <Metric
      label={label}
      value={
        <Link
          className="metric-link"
          to="/analytics"
          search={{ chart, range, ...(accountId ? { account: accountId } : {}) }}
          data-command={`navigation.report.${chart}`}
        >
          {formatCurrency(value)}
        </Link>
      }
    />
  )
}

/** An investment account's gains and income: what the range earned, then the totals. */
export function investmentMetrics({
  account,
  summary,
  range,
  rangeGain,
  contributions,
  unrealized,
  realized,
}: {
  account: Account
  summary: TransactionSummary
  range: ChartPeriod
  rangeGain: number | null
  contributions: number | null
  unrealized: { value: number | null | undefined; label: string }
  realized: number | null | undefined
}): ReactElement[] {
  return [
    <Metric
      key="gain"
      label="Investment gain"
      value={formatCurrency(rangeGain)}
      tone={valueTone(rangeGain)}
    />,
    <Metric key="contributions" label="Net contributions" value={formatCurrency(contributions)} />,
    <Metric key="cash" label="Cash" value={formatCurrency(account.cashValue)} />,
    <Metric
      key="unrealized"
      label={unrealized.label}
      value={formatCurrency(unrealized.value)}
      tone={valueTone(unrealized.value)}
    />,
    <Metric
      key="realized"
      label="Estimated realized P/L"
      value={formatCurrency(realized)}
      tone={valueTone(realized)}
    />,
    <ReportMetric
      key="dividends"
      chart="dividends"
      label="Dividends"
      value={summary.range.dividends}
      range={range}
      accountId={account.id}
    />,
    <ReportMetric
      key="interest"
      chart="interest"
      label="Interest"
      value={summary.range.interest}
      range={range}
      accountId={account.id}
    />,
  ]
}

/** A card's spending this month, then its statement, limit, and rates. */
export function creditMetrics({
  account,
  spending,
  transactionCount,
  card,
}: {
  account: Account
  spending: ReturnType<typeof buildSpendingView>
  transactionCount: number
  card: Insights['credit']['cards'][number] | undefined
}): ReactElement[] {
  const aprs = account.liability?.aprs?.filter(({ percentage }) => percentage != null) ?? []
  const purchaseApr = aprs.find(({ type }) => type === 'purchase_apr')
  const otherAprs = aprs
    .filter((apr) => apr !== purchaseApr)
    .map((apr) => `${aprLabels[apr.type] ?? apr.type} ${apr.percentage!.toFixed(2)}%`)
    .join(' · ')
  const liability = account.liability
  return [
    <Metric key="spent" label="Spent this month" value={formatCurrency(spending.total)} />,
    <Metric key="pending" label="Pending" value={formatCurrency(spending.pendingTotal)} />,
    <Metric
      key="largest"
      label="Largest expense"
      value={formatCurrency(spending.biggest ? Math.abs(spending.biggest.amount) : null)}
    />,
    <Metric key="transactions" label="Transactions" value={transactionCount} />,
    <Metric
      key="statement"
      label="Last statement"
      value={formatCurrency(liability?.lastStatementBalance)}
    />,
    <Metric
      key="due"
      label={
        liability?.nextPaymentDueDate
          ? `Due ${formatShortDay(liability.nextPaymentDueDate)}`
          : 'Payment due'
      }
      value={formatCurrency(liability?.minimumPayment)}
      tone={liability?.isOverdue ? 'negative' : undefined}
    />,
    <Metric
      key="limit"
      label="Limit used"
      value={
        card?.utilization != null ? (
          <>
            {formatShare(card.utilization)}
            <small className="metric-detail">of {formatCurrency(card.limit)}</small>
          </>
        ) : (
          'No preset limit'
        )
      }
      tone={card?.utilization != null ? undefined : 'muted'}
    />,
    ...(account.availableBalance != null
      ? [
          <Metric
            key="available"
            label="Available credit"
            value={formatCurrency(account.availableBalance)}
          />,
        ]
      : []),
    ...(card?.closesOn
      ? [
          <Metric
            key="closes"
            label="Statement closes"
            value={
              <>
                {`${card.closesEstimated ? '≈ ' : ''}${formatShortDay(card.closesOn)}`}
                {card.statementUtilization != null ? (
                  <small className="metric-detail">
                    {`last ${formatShare(card.statementUtilization)}`}
                  </small>
                ) : null}
              </>
            }
          />,
        ]
      : []),
    ...(purchaseApr
      ? [
          <Metric
            key="apr"
            label="Purchase APR"
            value={
              <>
                {`${purchaseApr.percentage!.toFixed(2)}%`}
                {otherAprs ? <small className="metric-detail">{otherAprs}</small> : null}
              </>
            }
          />,
        ]
      : []),
    ...(card?.interestCharged
      ? [
          <Metric
            key="interest"
            label="Interest charged"
            value={
              <>
                {formatCurrency(card.interestCharged)}
                <small className="metric-detail">last statement</small>
              </>
            }
            tone="negative"
          />,
        ]
      : []),
  ]
}

/**
 * A cash account's money in and out over the range, with the interest rate and usual monthly
 * flow Rust estimates for it.
 */
export function cashMetrics({
  account,
  summary,
  range,
  interest,
  apy,
}: {
  account: Account
  summary: TransactionSummary
  range: ChartPeriod
  interest: Insights['interest'][number] | undefined
  /** The APY the user recorded for the account, in percent. */
  apy?: number
}): ReactElement[] {
  const interestRate = interest?.periods.find(({ window }) => window === range)?.annualRate ?? null
  const averageMonthlyFlow = interest?.monthlyNetFlow ?? null
  return [
    ...(account.availableBalance != null && account.availableBalance !== account.value
      ? [
          <Metric
            key="available"
            label="Available"
            value={
              <>
                {formatCurrency(account.availableBalance)}
                <small className="metric-detail">after pending</small>
              </>
            }
          />,
        ]
      : []),
    <Metric key="in" label="Money in" value={formatCurrency(summary.range.moneyIn)} />,
    <Metric key="out" label="Money out" value={formatCurrency(summary.range.moneyOut)} />,
    <ReportMetric
      key="interest"
      chart="interest"
      label="Interest"
      value={summary.range.interest}
      range={range}
      accountId={account.id}
    />,
    apy == null ? (
      <Metric key="rate" label="Est. interest rate" value={formatShare(interestRate, 2)} />
    ) : (
      // The stated rate beside what posted interest shows the account earning.
      <Metric
        key="rate"
        label="APY"
        value={
          <>
            {formatShare(apy, 2)}
            <small className="metric-detail">
              {interestRate == null ? 'stated' : `earning ${formatShare(interestRate, 2)}`}
            </small>
          </>
        }
      />
    ),
    <Metric
      key="net"
      label="Net flow"
      value={formatCurrency(summary.range.netFlow)}
      tone={valueTone(summary.range.netFlow)}
    />,
    <Metric
      key="average"
      label="Monthly avg. net flow"
      value={
        averageMonthlyFlow == null ? (
          <span className="muted" title="Needs two months of history">
            —
          </span>
        ) : (
          formatCurrency(averageMonthlyFlow)
        )
      }
      tone={valueTone(averageMonthlyFlow)}
    />,
  ]
}
