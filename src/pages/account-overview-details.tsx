import type { Ref } from 'react'

import {
  cashMetrics,
  creditMetrics,
  investmentMetrics,
  ReportMetric,
} from '../components/account-metrics'
import { AccountStatements } from '../components/account-statements'
import { CashAheadChart } from '../components/charts'
import { ExternalLink } from '../components/external-link'
import { ArrowUpRight } from '../components/icons'
import { SettingsLink } from '../components/settings-dialog'
import { Card, Metric, SectionHeading } from '../components/ui'
import type { AccountGroup } from '../lib/account-groups'
import { addDays } from '../lib/dates'
import { financeSettings } from '../lib/finance-settings'
import {
  formatCurrency,
  formatCurrencyRange,
  formatShare,
  formatShares,
  formatShortDay,
  valueTone,
} from '../lib/format'
import type { ChartPeriod } from '../lib/graph-preferences'
import type { Account, FinanceSnapshot, Insights } from '../lib/schema'
import { currentSpendingPeriod } from '../lib/spending'
import { reconciliationSummary, reviewCount } from '../lib/statement-import'

// The figures an account shows before its Details disclosure.
const primaryMetricCount = 4

/** What moved through the view over the period, as Rust summed it. */
type AccountFlows = Omit<Insights['flows'][number], 'accountId' | 'window' | 'start' | 'end'>
/** Each figure is null when Rust has no flows for the view, so it reads as unknown, not $0. */
export type AccountFlowSummary = { [Key in keyof AccountFlows]: AccountFlows[Key] | null }

/**
 * The view's figures beside its chart: totals across every account, a type of account's
 * total and share, or one account's figures, each as Rust computed them.
 */
export function AccountOverviewDetails({
  account,
  group,
  isAll,
  data,
  committed,
  summary,
  period,
  isInvestment,
  isCredit,
  transactionCount,
  spendingAccountIds,
  detailsRef,
}: {
  account: Account
  group: AccountGroup | undefined
  isAll: boolean
  data: FinanceSnapshot
  committed: FinanceSnapshot
  summary: AccountFlowSummary
  period: ChartPeriod
  isInvestment: boolean
  isCredit: boolean
  transactionCount: number
  spendingAccountIds: string[]
  /** The Details disclosure, which the page's I key opens and closes. */
  detailsRef: Ref<HTMLDetailsElement>
}) {
  // Across every account, the gains Rust summed at the values shown.
  const rollup = data.insights?.rollup
  const gain = isAll ? (rollup?.unrealized ?? null) : account.knownUnrealizedGain
  const realizedGain = isAll ? (rollup?.realizedYtd ?? null) : account.estimatedRealizedGainYtd
  const unrealizedLabel =
    isAll || account.costBasisCoverage === 'partial' ? 'Known unrealized P/L' : 'Unrealized P/L'
  if (isAll) {
    const sheet = rollup?.sheet ?? { assets: 0, liabilities: 0, complete: false }
    // What the accounts own and owe, then investment and income figures across every account.
    // Group totals are in "Balances".
    return (
      <Card className="account-overview-summary-card">
        <SectionHeading title="Account details" />
        <div className="account-summary-metrics">
          <Metric
            label={sheet.complete ? 'Assets' : 'Known assets'}
            value={formatCurrency(sheet.assets)}
          />
          <Metric
            label={sheet.complete ? 'Liabilities' : 'Known liabilities'}
            value={formatCurrency(sheet.liabilities)}
            tone={sheet.liabilities < 0 ? 'negative' : undefined}
          />
          <ReportMetric chart="interest" label="Interest" value={summary.interest} range={period} />
          <ReportMetric
            chart="dividends"
            label="Dividends"
            value={summary.dividends}
            range={period}
          />
          <Metric label={unrealizedLabel} value={formatCurrency(gain)} tone={valueTone(gain)} />
          <Metric
            label="Estimated realized P/L"
            value={formatCurrency(realizedGain)}
            tone={valueTone(realizedGain)}
          />
          <Metric label="Net contributions" value={formatCurrency(summary.contributions)} />
          <Metric
            label="Investment gain"
            value={formatCurrency(summary.marketChange)}
            tone={valueTone(summary.marketChange)}
          />
        </div>
      </Card>
    )
  }
  if (group) {
    // A type of account: its total, its share of what you own, and how many accounts it holds,
    // each as Rust totaled it. Rust sums flows and gains for every account or one, not a type.
    const share = rollup?.shares.find(({ type }) => type === group.type)
    return (
      <Card className="account-overview-summary-card">
        <SectionHeading
          title={`${group.label} details`}
          detail={group.outsideTotal ? 'Spending cards, not in totals' : undefined}
        />
        <div className="account-summary-metrics">
          <Metric label="Total" value={formatCurrency(group.value)} />
          <Metric
            label="Change"
            value={
              group.change == null
                ? '—'
                : `${group.change > 0 ? '+' : ''}${formatCurrency(group.change)}`
            }
            tone={valueTone(group.change)}
          />
          <Metric label="Accounts" value={String(group.accounts.length)} />
          {share ? <Metric label="Share of assets" value={formatShare(share.percent)} /> : null}
          {group.type === 'cash' ? <CashAheadMetrics insights={data.insights} /> : null}
        </div>
        {group.type === 'cash' ? (
          <CashAheadDetails insights={data.insights} detailsRef={detailsRef} />
        ) : null}
      </Card>
    )
  }

  const accountDetails = financeSettings(committed).accountDetails?.[account.id]
  const ordered = accountMetrics({
    account,
    data,
    committed,
    summary,
    period,
    isInvestment,
    isCredit,
    transactionCount,
    gain,
    realizedGain,
    unrealizedLabel,
    apy: accountDetails?.apy,
  })
  const primaryMetrics = ordered.slice(0, primaryMetricCount)
  const moreMetrics = ordered.slice(primaryMetricCount)
  return (
    <Card className="account-overview-summary-card">
      <SectionHeading
        title="Account details"
        detail={
          [
            account.officialName,
            account.holderCategory === 'business' ? 'Business' : '',
            spendingAccountIds.includes(account.id) ? 'Spending card, not in totals' : '',
          ]
            .filter(Boolean)
            .join(' · ') || undefined
        }
        action={
          accountDetails?.website ? (
            <ExternalLink
              className="section-heading-link"
              href={accountDetails.website}
              data-command="account.website.open"
            >
              Website
              <ArrowUpRight size={12} aria-hidden="true" />
            </ExternalLink>
          ) : undefined
        }
      />
      <div className="account-summary-metrics">{primaryMetrics}</div>
      {moreMetrics.length ? (
        <details ref={detailsRef} className="account-details-more">
          <summary data-command="account.details.toggle">
            Details <small>{moreMetrics.length}</small>
          </summary>
          <div className="account-summary-metrics">{moreMetrics}</div>
        </details>
      ) : null}
      <AccountStatements accountId={account.id} />
    </Card>
  )
}

/**
 * One account's figures in the order it shows them. Each account type leads with the figures
 * that decide something; the rest stay one disclosure away, so every collected field remains
 * reachable without a wall of tiles.
 */
function accountMetrics({
  account,
  data,
  committed,
  summary,
  period,
  isInvestment,
  isCredit,
  transactionCount,
  gain,
  realizedGain,
  unrealizedLabel,
  apy,
}: {
  account: Account
  data: FinanceSnapshot
  committed: FinanceSnapshot
  summary: AccountFlowSummary
  period: ChartPeriod
  isInvestment: boolean
  isCredit: boolean
  transactionCount: number
  gain: number | null | undefined
  realizedGain: number | null | undefined
  unrealizedLabel: string
  apy: number | undefined
}) {
  const spending = currentSpendingPeriod(data.insights, account.id, 'month')
  const metrics = isInvestment
    ? investmentMetrics({
        account,
        summary,
        range: period,
        // One account's own market movement over the period.
        rangeGain: summary.marketChange,
        contributions: summary.contributions,
        unrealized: {
          value: gain,
          label:
            unrealizedLabel === 'Known unrealized P/L'
              ? 'Known total unrealized'
              : 'Total unrealized',
        },
        realized: realizedGain,
      })
    : isCredit
      ? creditMetrics({
          account,
          spending,
          largest: data.transactions.find(({ id }) => id === spending?.largest),
          transactionCount,
          card: data.insights?.credit.cards.find(({ accountId }) => accountId === account.id),
        })
      : cashMetrics({
          account,
          summary,
          range: period,
          interest: data.insights?.interest.find(({ accountId }) => accountId === account.id),
          apy,
        })
  // Statement reconciliation leads when something needs a look, and otherwise waits in Details.
  const reconciled = committed.statementReconciliation?.find(
    ({ accountId }) => accountId === account.id,
  )
  const statementMetric = reconciled ? (
    <StatementMetric key="statements" reconciled={reconciled} />
  ) : null
  const toReview = reconciled ? reviewCount(reconciled) : 0
  // A manual account's statement funds have no market price, so they lead its details rather
  // than appear among holdings; how the value is reached follows them.
  const positionMetrics = (account.statementPositions ?? []).map((position) => (
    <Metric
      key={`position:${position.name}`}
      label={position.name}
      value={
        <>
          {formatCurrency(position.value)}
          <small className="metric-detail">
            {formatShares(position.units)} units at {formatCurrency(position.price)} ·{' '}
            {formatShortDay(position.pricedOn)}
          </small>
        </>
      }
    />
  ))
  const valuationMetric =
    account.balanceSource === 'estimated' && account.balanceAsOf ? (
      <Metric
        key="valuation"
        label="Valuation"
        value={
          <>
            Est. since {formatShortDay(account.balanceAsOf)}
            <small className="metric-detail">moved with the S&amp;P 500</small>
          </>
        }
      />
    ) : null
  const withPositions = [
    ...positionMetrics,
    ...(valuationMetric ? [valuationMetric] : []),
    ...metrics,
  ]
  if (!statementMetric) return withPositions
  return toReview ? [statementMetric, ...withPositions] : [...withPositions, statementMetric]
}

const cashUnavailable = 'Unavailable while a cash balance is unknown'

/** Months of spending cash covers, as Rust rounded them down. */
const formatRunway = (months: number) => `${months.toFixed(1)} ${months === 1 ? 'month' : 'months'}`

/**
 * Cash ahead at a glance: the lowest the expected balance falls over the forecast, and how
 * many months of typical spending today's cash covers. Rust computes both; a value it can't
 * know reads as unavailable, never zero.
 */
function CashAheadMetrics({ insights }: { insights: Insights | undefined }) {
  const cash = insights?.cash
  const plan = insights?.plan
  const lowest = cash?.lowest
  return (
    <>
      <Metric
        label="Lowest ahead"
        value={
          !cash || cash.cashIncomplete ? (
            <span className="muted">Unavailable</span>
          ) : lowest ? (
            <>
              {formatCurrency(lowest.expected)}
              <small className="metric-detail">
                {formatShortDay(lowest.date)} · {formatCurrencyRange(lowest)}
              </small>
            </>
          ) : (
            <>
              {formatCurrency(cash.cash)}
              <small className="metric-detail">today; nothing expected lowers it</small>
            </>
          )
        }
        tone={lowest ? valueTone(lowest.expected) : undefined}
      />
      <Metric
        label="Runway"
        value={
          plan?.runwayMonths != null ? (
            <>
              {formatRunway(plan.runwayMonths)}
              <small className="metric-detail">
                of {formatCurrency(plan.monthlySpending)} a month
              </small>
            </>
          ) : (
            <span className="muted">Unavailable</span>
          )
        }
      />
    </>
  )
}

/**
 * The cash line behind its figures: the expected balance with its range, the low point, and
 * what drives it. Each bill and deposit is listed once, in Coming up beside this card.
 */
function CashAheadDetails({
  insights,
  detailsRef,
}: {
  insights: Insights | undefined
  detailsRef: Ref<HTMLDetailsElement>
}) {
  const cash = insights?.cash
  const today = insights?.today
  if (!cash || !today) return null
  const through = addDays(today, cash.horizonDays)
  const last = cash.days.at(-1)
  return (
    <details ref={detailsRef} className="account-details-more">
      <summary data-command="account.details.toggle">
        Cash ahead <small>{cash.horizonDays} days</small>
      </summary>
      {cash.cashIncomplete || !last ? (
        <p className="metric-detail">{cashUnavailable}.</p>
      ) : (
        <>
          <div className="account-summary-metrics">
            <Metric
              label={`By ${formatShortDay(through)}`}
              value={
                <>
                  {formatCurrency(last.expected)}
                  <small className="metric-detail">range {formatCurrencyRange(last)}</small>
                </>
              }
            />
            <Metric label="Expected in" value={formatCurrency(cash.inflows)} />
            <Metric label="Expected out" value={formatCurrency(cash.outflows)} />
            <Metric
              label="Pending"
              value={
                <>
                  {formatCurrency(cash.pendingChange)}
                  <small className="metric-detail">already in today’s cash</small>
                </>
              }
            />
          </div>
          <CashAheadChart days={cash.days} through={through} lowest={cash.lowest} />
          <p className="metric-detail">
            Recurring bills and deposits to cash accounts, each listed in Coming up; the range runs
            from the smallest to the largest amount each has posted.
          </p>
        </>
      )}
    </details>
  )
}

/** How the account's imported statements compare with Plaid; opens Statement history. */
function StatementMetric({
  reconciled,
}: {
  reconciled: NonNullable<FinanceSnapshot['statementReconciliation']>[number]
}) {
  const { headline, detail, review } = reconciliationSummary(reconciled)
  return (
    <Metric
      label="Statements"
      value={
        <SettingsLink target="statement-history" className={review ? 'warning' : undefined}>
          {headline}
          <small className="metric-detail">{detail}</small>
        </SettingsLink>
      }
    />
  )
}
