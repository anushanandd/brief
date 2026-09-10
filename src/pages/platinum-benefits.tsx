import { useMutation, useQueryClient } from '@tanstack/react-query'
import { ExternalLink } from 'lucide-react'
import { useState } from 'react'
import { toast } from 'sonner'

import { PageError, PageLoading } from '../components/data-state'
import { ActivityDetailHeader } from '../components/spending-detail-header'
import { TransactionList } from '../components/transaction-list'
import { Card, SectionHeading } from '../components/ui'
import { useFinance } from '../hooks/use-finance'
import { accountDisplayName, getAccountDisplayNames } from '../lib/account-name-preferences'
import { saveTransactionAnnotation } from '../lib/annotations'
import { formatCompactCurrency, formatCurrency } from '../lib/format'
import {
  buildPlatinumBenefitHistory,
  buildPlatinumBenefitTracker,
  getPlatinumBenefitActivity,
  platinumBenefitOptions,
  resolveSpendingAccount,
  transactionDateKey,
} from '../lib/spending'
import { getHiddenPlatinumBenefitIds, getSpendingAccountId } from '../lib/spending-preferences'

const monthNames = [
  'Jan',
  'Feb',
  'Mar',
  'Apr',
  'May',
  'Jun',
  'Jul',
  'Aug',
  'Sep',
  'Oct',
  'Nov',
  'Dec',
]

export function PlatinumBenefitsPage() {
  const query = useFinance()
  const client = useQueryClient()
  const confirmation = useMutation({
    mutationFn: ({ id, confirmed }: { id: string; confirmed: boolean }) =>
      saveTransactionAnnotation(id, { benefitConfirmed: confirmed }),
    onSuccess: (annotations) => client.setQueryData(['transaction-annotations'], { annotations }),
    onError: () => toast.error('Could not save reimbursement confirmation'),
  })
  const [year, setYear] = useState<number | null>(null)
  const [benefitId, setBenefitId] = useState('')
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />

  const data = query.data
  const account = resolveSpendingAccount(data.accounts, getSpendingAccountId())
  const accountName = account
    ? accountDisplayName(account.id, account.name, getAccountDisplayNames())
    : undefined
  const transactions = account
    ? data.transactions.filter(({ accountId }) => accountId === account.id)
    : []
  const referenceDay = transactionDateKey(data.updatedAt, data.updatedAt)
  const dates = transactions
    .map(({ date }) => transactionDateKey(date, data.updatedAt))
    .filter((date) => date && date <= referenceDay)
    .toSorted()
  const currentYear = Number(referenceDay.slice(0, 4))
  const years = [
    ...new Set([currentYear, ...dates.map((date) => Number(date.slice(0, 4)))]),
  ].toSorted((left, right) => right - left)
  const selectedYear = year !== null && years.includes(year) ? year : currentYear
  const hiddenIds = getHiddenPlatinumBenefitIds()
  const trackedBenefits = platinumBenefitOptions.filter(({ id }) => !hiddenIds.includes(id))
  const selectedBenefitId = trackedBenefits.some(({ id }) => id === benefitId) ? benefitId : ''
  const activity = getPlatinumBenefitActivity(transactions, data.updatedAt).filter(
    ({ benefitId: id }) =>
      !hiddenIds.includes(id) && (!selectedBenefitId || id === selectedBenefitId),
  )
  const history = buildPlatinumBenefitHistory(activity, selectedYear)
  const benefits = history.benefits.filter(
    ({ id }) => !hiddenIds.includes(id) && (!selectedBenefitId || id === selectedBenefitId),
  )
  const recordedMonths = new Set(dates.map((date) => date.slice(0, 7)))
  const currentTracker = buildPlatinumBenefitTracker(transactions, data.updatedAt).filter(
    ({ id }) => !hiddenIds.includes(id),
  )
  const nextBenefit = currentTracker.find(({ remainingAmount }) => remainingAmount > 0)
  const detectedCount = currentTracker.filter(({ status }) => status === 'detected').length
  const completedCount = currentTracker.filter(
    ({ remainingAmount }) => remainingAmount === 0,
  ).length
  const maxMonthlyCredit = Math.max(
    1,
    ...history.months.map(({ creditedAmount }) => creditedAmount),
  )

  return (
    <div className="page spending-detail-page">
      <ActivityDetailHeader title="Platinum benefits" accountName={accountName} />
      <div className="spending-history-toolbar benefits-history-toolbar">
        <div className="spending-history-filters">
          <label className="spending-history-filter">
            <span>Year</span>
            <select value={selectedYear} onChange={(event) => setYear(Number(event.target.value))}>
              {years.map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
          </label>
          <label className="spending-history-filter">
            <span>Benefit</span>
            <select
              value={selectedBenefitId}
              onChange={(event) => setBenefitId(event.target.value)}
            >
              <option value="">All tracked benefits</option>
              {trackedBenefits.map(({ id, name }) => (
                <option key={id} value={id}>
                  {name}
                </option>
              ))}
            </select>
          </label>
        </div>
        <a
          className="platinum-benefits-link"
          href="https://global.americanexpress.com/card-benefits/activity"
          target="_blank"
          rel="noreferrer"
        >
          Manage in Amex <ExternalLink size={12} aria-hidden="true" />
        </a>
      </div>

      <Card className="benefit-attention-card">
        <div className="benefit-attention-primary">
          <span className="balance-label">Next tracked opportunity</span>
          <strong>{nextBenefit?.name ?? 'All current benefits confirmed'}</strong>
          <p>
            {nextBenefit
              ? `${formatCurrency(nextBenefit.remainingAmount)} not confirmed · resets ${nextBenefit.reset}`
              : 'No remaining value in the benefits currently shown.'}
          </p>
        </div>
        <div className="benefit-attention-metrics">
          <span>
            <strong>{detectedCount}</strong>
            <small>Need review</small>
          </span>
          <span>
            <strong>{completedCount}</strong>
            <small>Fully confirmed</small>
          </span>
        </div>
      </Card>

      <Card className="spending-detail-card benefit-history-summary">
        <span className="balance-label">Matched credits · {selectedYear}</span>
        <strong className="spending-balance">{formatCurrency(history.creditedAmount)}</strong>
        <p className="spending-detail-note" role="status">
          {history.creditCount} posted credits across{' '}
          {benefits.filter(({ creditCount }) => creditCount > 0).length} benefits
        </p>
        <div
          className="benefit-month-chart"
          role="list"
          aria-label={`Monthly matched credits for ${selectedYear}`}
        >
          {history.months.map(({ month, creditedAmount }, index) => {
            const hasRecords = recordedMonths.has(month)
            const label = `${monthNames[index]}: ${hasRecords ? formatCurrency(creditedAmount) : 'no imported transactions'}`
            return (
              <div
                className="benefit-month-column"
                key={month}
                role="listitem"
                aria-label={label}
                title={label}
              >
                <span className="benefit-month-value" aria-hidden="true">
                  {hasRecords ? formatCompactCurrency(creditedAmount) : '—'}
                </span>
                <div className="benefit-month-track" aria-hidden="true">
                  <div
                    className="benefit-month-bar"
                    style={{ height: `${(creditedAmount / maxMonthlyCredit) * 100}%` }}
                  />
                </div>
                <span aria-hidden="true">{monthNames[index]}</span>
              </div>
            )
          })}
        </div>
        <p className="spending-detail-note">
          — means no imported transactions for that month. Available history may be partial.
        </p>
      </Card>

      <Card className="spending-detail-card">
        <SectionHeading title="By benefit" detail={`Matched credits in ${selectedYear}`} />
        {benefits.length ? (
          <table className="benefit-history-table">
            <thead>
              <tr>
                <th scope="col">Benefit</th>
                <th scope="col">Posted credits</th>
                <th scope="col">Amount</th>
              </tr>
            </thead>
            <tbody>
              {benefits.map(({ id, name, creditCount, creditedAmount }) => (
                <tr key={id}>
                  <th scope="row">
                    <button type="button" onClick={() => setBenefitId(id)}>
                      {name}
                    </button>
                  </th>
                  <td>{creditCount}</td>
                  <td className={creditedAmount > 0 ? 'positive' : undefined}>
                    {formatCurrency(creditedAmount)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ) : (
          <p className="spending-detail-note">
            All benefits are hidden. Choose benefits to track in Settings.
          </p>
        )}
      </Card>

      <Card className="spending-detail-card">
        <SectionHeading
          title="Matched activity"
          detail="Purchases, credits and pending activity for the selected year and benefit."
        />
        <TransactionList
          transactions={history.activity.map((transaction) => ({
            ...transaction,
            category: transaction.benefitName,
          }))}
          referenceIso={data.updatedAt}
          detailed
          savingConfirmation={confirmation.isPending}
          onBenefitConfirmation={(id, confirmed) => confirmation.mutate({ id, confirmed })}
        />
      </Card>
      <p className="spending-detail-note">
        Matches use merchant names and descriptions and can include refunds. Totals exclude pending
        credits and do not confirm benefit reimbursement. Historical amounts are not capped using
        today’s benefit limits.
        {dates.length
          ? ` Imported transactions: ${dates[0]} through ${dates.at(-1)}.`
          : ' No transaction history imported yet.'}
      </p>
    </div>
  )
}
