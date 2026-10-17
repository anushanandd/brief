import { toast } from 'sonner'

import { useFinanceEdits } from '../hooks/use-finance-edits'
import { accountDisplayName } from '../lib/account-names'
import { errorMessage } from '../lib/errors'
import { financeSettings } from '../lib/finance-settings'
import { formatCurrency, formatProsePercent, formatShortDay } from '../lib/format'
import type { FinanceSnapshot, Retirement } from '../lib/schema'
import { AccountMark } from './account-mark'
import { FilterSelect } from './filter-select'
import { SettingsLink } from './settings-dialog'
import { Card, EmptyState, Metric, SectionHeading } from './ui'

/** Years of spending at which the 4% rule says a portfolio can fund it indefinitely. */
const FI_YEARS = 25

type Names = (id: string) => string

/** One limit: what counts against it, how much of it that is, and what's next. */
function LimitRow({
  title,
  used,
  limit,
  details,
}: {
  title: string
  used: number
  limit?: number
  details: string[]
}) {
  const share = limit ? Math.min(used / limit, 1) : 0
  return (
    <li className="retirement-limit">
      <span className="retirement-limit-heading">
        <strong>{title}</strong>
        <span>
          <span className="sensitive-value">{formatCurrency(used)}</span>
          {limit != null ? (
            <small>
              {' of '}
              {formatCurrency(limit)}
            </small>
          ) : null}
        </span>
      </span>
      {limit != null ? (
        <span
          className="retirement-limit-track"
          role="meter"
          aria-label={`${title} used`}
          aria-valuemin={0}
          aria-valuemax={limit}
          aria-valuenow={Math.min(used, limit)}
        >
          <span style={{ width: `${share * 100}%` }} />
        </span>
      ) : null}
      {details.length ? <small>{details.join(' · ')}</small> : null}
    </li>
  )
}

const unpublished = (year: number) => `${year} limits aren’t published yet`

function Contributions({ retirement, name }: { retirement: Retirement; name: Names }) {
  const { workplace, ira, uncounted } = retirement
  if (!workplace && !ira.length)
    return (
      <Card aria-label="Contributions">
        <SectionHeading title="Contributions" />
        <EmptyState>
          No 401(k) or IRA. Name a retirement account with its plan, such as “401(k)” or “Roth IRA”.
        </EmptyState>
      </Card>
    )
  return (
    <Card aria-label="Contributions">
      <SectionHeading title="Contributions" />
      <ul className="retirement-limits">
        {workplace ? (
          <LimitRow
            title={`${workplace.accountIds.length > 1 ? 'Workplace plans' : name(workplace.accountIds[0])} · ${retirement.year}`}
            used={workplace.deferred}
            limit={workplace.limit}
            details={[
              workplace.limit == null ? unpublished(retirement.year) : '',
              workplace.fromPayStatements
                ? 'From your latest pay statement'
                : workplace.through
                  ? `Through ${formatShortDay(workplace.through)}`
                  : 'Nothing imported this year',
              workplace.projected != null
                ? `On pace for ${formatCurrency(workplace.projected)}`
                : '',
              workplace.perPaycheckToMax != null && workplace.paychecksLeft
                ? `${formatCurrency(workplace.perPaycheckToMax)} a paycheck to max, ${workplace.paychecksLeft} left`
                : '',
              workplace.employer > 0
                ? `${formatCurrency(workplace.employer)} from your employer, outside the limit`
                : '',
              workplace.unlabeled > 0
                ? `${formatCurrency(workplace.unlabeled)} not labeled yours or your employer’s`
                : '',
            ].filter(Boolean)}
          />
        ) : null}
        {ira.map((year) => (
          <LimitRow
            key={year.taxYear}
            title={`IRA · ${year.taxYear}`}
            used={year.contributed}
            limit={year.limit}
            details={[
              year.limit == null
                ? unpublished(year.taxYear)
                : `${formatCurrency(Math.max(year.limit - year.contributed, 0))} left`,
              `Until ${formatShortDay(year.deadline)}${
                year.deadline.slice(0, 4) === String(retirement.year)
                  ? ''
                  : `, ${year.deadline.slice(0, 4)}`
              }`,
            ]}
          />
        ))}
      </ul>
      <Deposits retirement={retirement} name={name} />
      {uncounted.length ? (
        <p className="coming-up-quiet">
          Not counted toward a limit: {uncounted.map(name).join(', ')}
        </p>
      ) : null}
    </Card>
  )
}

const flowLabels: Record<string, string> = {
  contribution: 'Contribution',
  deposit: 'Deposit',
  transfer: 'Transfer in',
}

/**
 * IRA deposits and where each counts. One Brief can't place waits for a choice; any can be
 * changed, and Automatic hands it back to Brief.
 */
function Deposits({ retirement, name }: { retirement: Retirement; name: Names }) {
  const { updateSettings } = useFinanceEdits()
  if (!retirement.deposits.length) return null
  const place = (transactionId: string, choice: string) =>
    void updateSettings(({ retirement: profile = {}, ...current }) => {
      const { [transactionId]: _previous, ...choices } = profile.contributionYears ?? {}
      return {
        ...current,
        retirement: {
          ...profile,
          contributionYears:
            choice === ''
              ? choices
              : { ...choices, [transactionId]: choice === 'none' ? null : Number(choice) },
        },
      }
    }).catch((error) =>
      toast.error('Could not save the contribution', { description: errorMessage(error) }),
    )
  return (
    <>
      <h3 className="retirement-subheading">IRA deposits</h3>
      <ul className="coming-up-list" data-keyboard-region aria-label="IRA deposits">
        {retirement.deposits.toReversed().map((deposit) => {
          const automatic = deposit.waiting
            ? 'Choose'
            : deposit.countsFor == null
              ? 'Not a contribution'
              : `Counts for ${deposit.countsFor}`
          return (
            <li
              className="coming-up-row retirement-inflow"
              data-keyboard-row
              key={deposit.transactionId}
            >
              <time dateTime={deposit.date}>{formatShortDay(deposit.date)}</time>
              <AccountMark type="retirement" />
              <span className="coming-up-copy">
                <strong>{name(deposit.accountId)}</strong>
                <small>
                  {deposit.waiting
                    ? deposit.years.length > 1
                      ? `Can count for ${deposit.years.join(' or ')}`
                      : 'A contribution, or a rollover?'
                    : deposit.stated
                      ? `${flowLabels[deposit.flow] ?? deposit.flow} · the statement says ${deposit.countsFor}`
                      : (flowLabels[deposit.flow] ?? deposit.flow)}
                </small>
              </span>
              <span className="sensitive-value" data-copy-amount>
                {formatCurrency(deposit.amount)}
              </span>
              <FilterSelect
                label={`Where the ${formatShortDay(deposit.date)} deposit counts`}
                keyboardOpen
                value={
                  deposit.chosen
                    ? deposit.countsFor == null
                      ? 'none'
                      : String(deposit.countsFor)
                    : ''
                }
                options={[
                  { value: '', label: deposit.chosen ? 'Automatic' : automatic },
                  // Brief's own placement already reads as the first option.
                  ...deposit.years
                    .toReversed()
                    .filter((year) => deposit.chosen || year !== deposit.countsFor)
                    .map((year) => ({ value: String(year), label: `Counts for ${year}` })),
                  { value: 'none', label: 'Not a contribution' },
                ]}
                onValueChange={(choice) => place(deposit.transactionId, choice)}
              />
            </li>
          )
        })}
      </ul>
    </>
  )
}

function Savings({ retirement }: { retirement: Retirement }) {
  const [current, ...earlier] = retirement.savings
  return (
    <Card className="investments-side-card" aria-label="Savings rate">
      <SectionHeading title="Savings rate" />
      {current ? (
        <div className="investments-side-stats">
          <Metric
            label={
              current.partial ? `Since ${formatShortDay(current.from)}` : `${current.year} so far`
            }
            value={current.rate == null ? 'No pay yet' : formatProsePercent(current.rate)}
            tone={current.rate == null ? 'muted' : undefined}
          />
          {earlier.map((year) => (
            <Metric
              key={year.year}
              label={String(year.year)}
              value={year.rate == null ? 'No pay' : formatProsePercent(year.rate)}
              tone="muted"
            />
          ))}
          <Metric
            label="Take-home pay"
            value={<span className="sensitive-value">{formatCurrency(current.takeHome)}</span>}
          />
          {current.deferred > 0 ? (
            <Metric
              label="401(k) from pay"
              value={<span className="sensitive-value">{formatCurrency(current.deferred)}</span>}
            />
          ) : null}
          {current.espp > 0 ? (
            <Metric
              label="Stock purchases from pay"
              value={<span className="sensitive-value">{formatCurrency(current.espp)}</span>}
            />
          ) : null}
          <Metric
            label="Spent"
            value={<span className="sensitive-value">{formatCurrency(current.spending)}</span>}
          />
        </div>
      ) : (
        <EmptyState>No activity yet.</EmptyState>
      )}
    </Card>
  )
}

function Buckets({ retirement, name }: { retirement: Retirement; name: Names }) {
  const { buckets, rothBasis } = retirement
  return (
    <Card aria-label="By tax treatment">
      <SectionHeading title="By tax treatment" />
      <div className="account-summary-metrics retirement-metrics">
        <Metric
          label="Pre-tax"
          value={<span className="sensitive-value">{formatCurrency(buckets.preTax)}</span>}
        />
        <Metric
          label="Roth and HSA"
          value={<span className="sensitive-value">{formatCurrency(buckets.taxFree)}</span>}
        />
        <Metric
          label="Taxable"
          value={<span className="sensitive-value">{formatCurrency(buckets.taxable)}</span>}
        />
      </div>
      {rothBasis.map((basis) => (
        <p className="coming-up-quiet" key={basis.accountId}>
          {name(basis.accountId)}:{' '}
          <span className="sensitive-value">{formatCurrency(basis.basis)}</span> in contributions
          you can withdraw anytime, tax- and penalty-free.{' '}
          {basis.earlier == null && basis.since ? (
            <SettingsLink target="retirement-earlier-contributions">
              Add any from before {formatShortDay(basis.since)}
            </SettingsLink>
          ) : null}
        </p>
      ))}
      {buckets.complete ? null : (
        <p className="coming-up-quiet">An account has no value, so these are incomplete.</p>
      )}
    </Card>
  )
}

function Coverage({ data, retirement }: { data: FinanceSnapshot; retirement: Retirement }) {
  const { coverage } = retirement
  const planned = financeSettings(data).retirement?.plannedMonthly
  return (
    <Card aria-label="Years of spending covered">
      <SectionHeading title="Years covered" />
      {coverage ? (
        <>
          <div className="account-summary-metrics retirement-metrics">
            <Metric
              label={`Of ${FI_YEARS} for the 4% rule`}
              value={`${coverage.years.toFixed(1)} years`}
            />
            <Metric
              label="Invested"
              value={<span className="sensitive-value">{formatCurrency(coverage.invested)}</span>}
            />
            <Metric
              label={coverage.planned ? 'A year, with planned costs' : 'A year of spending'}
              value={
                <span className="sensitive-value">
                  {formatCurrency(coverage.spending + coverage.planned)}
                </span>
              }
            />
          </div>
          {coverage.complete && planned ? null : (
            <p className="coming-up-quiet">
              {coverage.complete ? null : 'An account has no value, so this is low. '}
              {planned ? null : (
                <SettingsLink target="retirement-planned-costs">
                  Add costs you’ll take on, such as rent
                </SettingsLink>
              )}
            </p>
          )}
        </>
      ) : (
        <EmptyState>Needs a full year of spending history.</EmptyState>
      )}
    </Card>
  )
}

/** Retirement: this year's contributions against their limits, then how saving is going. */
export function RetirementView({ data }: { data: FinanceSnapshot }) {
  const retirement = data.insights?.retirement
  if (!retirement)
    return (
      <Card>
        <EmptyState>Retirement figures are unavailable.</EmptyState>
      </Card>
    )
  const names = financeSettings(data).accountNames
  const name = (id: string) =>
    accountDisplayName(
      id,
      data.accounts.find((account) => account.id === id)?.name ?? 'Account',
      names,
    )
  return (
    <>
      <div className="investments-hero">
        <div className="investments-main">
          <Contributions retirement={retirement} name={name} />
        </div>
        <div className="investments-side">
          <Savings retirement={retirement} />
        </div>
      </div>
      <div className="investments-pair">
        <Buckets retirement={retirement} name={name} />
        <Coverage data={data} retirement={retirement} />
      </div>
    </>
  )
}
