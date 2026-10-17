import { Popover } from '@base-ui/react/popover'
import { Link } from '@tanstack/react-router'
import type { ReactNode } from 'react'

import { useFinance } from '../hooks/use-finance'
import {
  benefitNote,
  benefitStatus,
  benefitWarning,
  creditLagNote,
  purchaseCreditLabel,
} from '../lib/benefit-copy'
import { financeSettings } from '../lib/finance-settings'
import { formatActivityName, formatCurrency, formatDay, formatProseCurrency } from '../lib/format'
import { brandLogoUrl, brandMarkLabel, getExternalLogosEnabled } from '../lib/logos'
import type { BenefitActivity, BenefitWindow, Insights } from '../lib/schema'
import { benefitReviewItems, benefitsToUseFirst, formatActivityDate } from '../lib/spending'
import { BrandMark } from './brand-mark'
import { ExternalLink } from './external-link'
import {
  BedDouble,
  ChevronRight,
  History,
  Info,
  MonitorPlay,
  Plane,
  ReceiptText,
  X,
  type IconComponent,
} from './icons'
import { Button, Card, EmptyState, SectionHeading } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

type Benefit = BenefitWindow
type BenefitEvidenceItem = BenefitActivity

const benefitLogoDomains: Record<string, string> = {
  'uber-cash': 'uber.com',
  'uber-one': 'uber.com',
  'walmart-plus': 'walmart.com',
  resy: 'resy.com',
  lululemon: 'lululemon.com',
  clear: 'clearme.com',
  oura: 'ouraring.com',
  equinox: 'equinox.com',
  'global-entry': 'cbp.gov',
  soulcycle: 'soul-cycle.com',
  saks: 'saksfifthavenue.com',
}

const benefitIcons: Partial<Record<string, { Icon: IconComponent; tone: string }>> = {
  'digital-entertainment': { Icon: MonitorPlay, tone: 'entertainment' },
  hotel: { Icon: BedDouble, tone: 'hotel' },
  'airline-fee': { Icon: Plane, tone: 'airline' },
}

export function BenefitMark({
  id,
  name,
  externalLogosEnabled,
}: {
  id: string
  name: string
  externalLogosEnabled: boolean
}) {
  const icon = benefitIcons[id]
  if (icon) {
    const { Icon, tone } = icon
    return (
      <span
        className={`transaction-mark benefit-mark benefit-mark-${tone}`}
        role="img"
        aria-label={`${name} benefit icon`}
      >
        <Icon size={15} aria-hidden="true" />
      </span>
    )
  }
  return (
    <BrandMark
      className="transaction-mark benefit-mark"
      fallback={brandMarkLabel(name)}
      label={`${name} logo`}
      src={brandLogoUrl(name, benefitLogoDomains[id], externalLogosEnabled)}
    />
  )
}

function BenefitProgress({ benefit }: { benefit: Benefit }) {
  const unavailable = !benefit.remainingRange && benefit.estimatedCreditAmount === null
  const value = benefit.estimatedCreditAmount ?? benefit.creditedAmount
  return (
    <progress
      className="benefit-progress"
      max={benefit.cap}
      value={unavailable ? 0 : Math.min(benefit.cap, Math.max(0, value))}
      aria-label={`${benefit.name} credit progress`}
      aria-valuetext={
        benefit.estimatedCreditAmount !== null
          ? benefit.estimatedCreditAmount > 0
            ? `Estimated ${formatCurrency(benefit.estimatedCreditAmount)} used`
            : 'No Uber purchase detected'
          : unavailable
            ? 'Unavailable'
            : undefined
      }
    />
  )
}

function BenefitInfo({
  title,
  command,
  children,
}: {
  title: string
  /** The registered action that opens it, for a trigger outside a list row. */
  command?: string
  children: ReactNode
}) {
  return (
    <Popover.Root>
      <Popover.Trigger
        aria-label={`About ${title}`}
        data-command={command}
        data-keyboard-open
        render={
          <Button size="icon" variant="ghost" className="icon-only-subtle benefit-info-trigger" />
        }
      >
        <Info size={14} aria-hidden="true" />
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Positioner
          side="bottom"
          align="start"
          sideOffset={8}
          className="benefit-info-positioner"
        >
          <Popover.Popup className="benefit-info-popup" data-popup-open="" initialFocus>
            <div className="benefit-info-heading">
              <Popover.Title>{title}</Popover.Title>
              <Popover.Close
                aria-label={`Close ${title} information`}
                render={<Button size="icon-compact" variant="ghost" />}
              >
                <X size={14} aria-hidden="true" />
              </Popover.Close>
            </div>
            {children}
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  )
}

function BenefitEvidence({
  activity,
  externalLogosEnabled,
}: {
  activity: BenefitEvidenceItem[]
  externalLogosEnabled?: boolean
}) {
  const labels = {
    credit: 'Benefit credit',
    reversal: 'Credit reversal',
    purchase: '',
    unverified: 'Unverified credit',
  }
  return (
    <ul className="benefit-evidence" aria-label="Transaction evidence">
      {activity.map((item) => (
        <li key={item.id} className={externalLogosEnabled === undefined ? undefined : 'with-mark'}>
          {externalLogosEnabled === undefined ? null : (
            <BenefitMark
              id={item.benefitId}
              name={item.benefitName}
              externalLogosEnabled={externalLogosEnabled}
            />
          )}
          <span>
            <span>{formatActivityName(item.merchant)}</span>
            <time dateTime={item.date}>{formatActivityDate(item.date)}</time>
            {labels[item.kind] || item.pending || item.credit ? (
              <small>
                {[
                  item.kind === 'credit' && !item.pending ? '' : labels[item.kind],
                  item.pending ? 'Pending' : '',
                  purchaseCreditLabel(item.credit),
                ]
                  .filter(Boolean)
                  .join(' · ')}
              </small>
            ) : null}
            {item.description && item.description !== item.merchant ? (
              <small>{item.description}</small>
            ) : null}
          </span>
          <strong className={item.kind === 'credit' && !item.pending ? 'positive' : undefined}>
            {formatCurrency(item.amount)}
          </strong>
        </li>
      ))}
    </ul>
  )
}

function BenefitRow({
  benefit,
  externalLogosEnabled,
  compact = false,
}: {
  benefit: Benefit
  externalLogosEnabled: boolean
  compact?: boolean
}) {
  const hasPostedCredits = benefit.activity.some(
    ({ kind, pending }) => !pending && (kind === 'credit' || kind === 'reversal'),
  )
  const supportingActivity = benefit.activity.filter(
    ({ kind, pending }) => pending || (kind !== 'credit' && kind !== 'reversal'),
  )
  const qualifyingPurchases = supportingActivity.filter(({ kind }) => kind === 'purchase')
  const visibleSupportingActivity = supportingActivity.filter(({ kind }) => kind !== 'purchase')
  const lastCreditLabel = benefit.lastCredit
    ? `Last detected credit: ${formatCurrency(benefit.lastCredit.amount)} on ${formatActivityDate(benefit.lastCredit.date)}.`
    : null
  const status = benefitStatus(benefit)
  const warning = benefitWarning(benefit)

  return (
    // A row in the list's keyboard model: J and K move through benefits, Return opens About.
    <article
      className="spending-benefit-row"
      aria-labelledby={`benefit-${benefit.id}`}
      tabIndex={-1}
      data-keyboard-row
    >
      <div className="benefit-detail-row">
        <BenefitMark
          id={benefit.id}
          name={benefit.name}
          externalLogosEnabled={externalLogosEnabled}
        />
        <div className="benefit-title-row">
          <h4 id={`benefit-${benefit.id}`}>{benefit.name}</h4>
          <BenefitInfo title={benefit.name}>
            <p>{benefit.instruction}</p>
            {benefit.deadlineNote ? <p>Cutoff: {benefit.deadlineNote}.</p> : null}
            {benefit.cadence === 'renewal' ? <p>Based on the last posted credit.</p> : null}
            {hasPostedCredits || qualifyingPurchases.length ? (
              <p>Credits post: {creditLagNote(benefit)}</p>
            ) : null}
            <ExternalLink className="benefit-terms-link" href={benefit.url}>
              Official benefit terms
            </ExternalLink>
          </BenefitInfo>
        </div>
        <strong className="benefit-allowance">
          {compact ? formatCurrency(benefit.cap) : benefit.limit}
        </strong>
        <BenefitProgress benefit={benefit} />
        <span
          className={
            status.tone === 'positive' ? 'benefit-detail-status positive' : 'benefit-detail-status'
          }
        >
          {status.text}
          {lastCreditLabel ? (
            <Tooltip>
              <TooltipTrigger
                render={
                  <Button
                    type="button"
                    size="icon-compact"
                    variant="ghost"
                    className="benefit-evidence-trigger"
                    aria-label={lastCreditLabel}
                  />
                }
              >
                <History size={14} aria-hidden="true" />
              </TooltipTrigger>
              <TooltipContent className="benefit-last-credit-tooltip" sideOffset={6}>
                {lastCreditLabel}
              </TooltipContent>
            </Tooltip>
          ) : null}
          {qualifyingPurchases.length ? (
            <Tooltip>
              <TooltipTrigger
                render={
                  <Button
                    type="button"
                    size="icon-compact"
                    variant="ghost"
                    className="benefit-evidence-trigger"
                    aria-label={`Purchase evidence for ${benefit.name}`}
                  />
                }
              >
                <ReceiptText size={14} aria-hidden="true" />
              </TooltipTrigger>
              <TooltipContent className="benefit-evidence-tooltip" sideOffset={6}>
                <BenefitEvidence activity={qualifyingPurchases} />
              </TooltipContent>
            </Tooltip>
          ) : null}
        </span>
        <small className={benefit.remainingRange ? 'benefit-remaining' : undefined}>
          {benefitNote(benefit)}
        </small>
        {warning ? <p className="benefit-warning">{warning}</p> : null}
      </div>
      {visibleSupportingActivity.length && benefit.status !== 'external' ? (
        <BenefitEvidence activity={visibleSupportingActivity} />
      ) : null}
    </article>
  )
}

/** Renewal and per-purchase benefits have no calendar deadline. */
const special = ({ cadence }: Benefit) => cadence === 'renewal' || cadence === 'purchase'

function BenefitGroups({
  benefits,
  externalLogosEnabled,
  compact = false,
}: {
  benefits: Benefit[]
  externalLogosEnabled: boolean
  compact?: boolean
}) {
  const calendarBenefits = benefits.filter((benefit) => !special(benefit) && !benefit.recurring)
  // Subscriptions spend these on their own: there is no deadline to act on, only a credit to check.
  const recurringBenefits = benefits.filter((benefit) => !special(benefit) && benefit.recurring)
  const specialBenefits = benefits.filter(special)
  const deadlines = [...new Set(calendarBenefits.map(({ windowEnd }) => windowEnd))].toSorted()

  return (
    <>
      {deadlines.map((deadline) => {
        // Rows keep the order they arrive in: Rust's use-first order, then the rest.
        const group = calendarBenefits.filter(({ windowEnd }) => windowEnd === deadline)
        const first = group[0]
        return (
          <section
            className="benefit-deadline-group"
            key={deadline}
            aria-label={`Benefits ending ${first.reset}`}
            data-keyboard-region
          >
            <h3>
              Ending {first.reset}
              <span className={first.daysRemaining <= 14 ? 'benefit-ending-soon' : undefined}>
                {first.daysRemaining === 0 ? 'Today' : `${first.daysRemaining} days left`}
              </span>
            </h3>
            {group.map((benefit) => (
              <BenefitRow
                key={benefit.id}
                benefit={benefit}
                externalLogosEnabled={externalLogosEnabled}
                compact={compact}
              />
            ))}
          </section>
        )
      })}
      {recurringBenefits.length ? (
        <section
          className="benefit-deadline-group"
          aria-label="Recurring credits"
          data-keyboard-region
        >
          <h3>Recurring</h3>
          {recurringBenefits.map((benefit) => (
            <BenefitRow
              key={benefit.id}
              benefit={benefit}
              externalLogosEnabled={externalLogosEnabled}
              compact={compact}
            />
          ))}
        </section>
      ) : null}
      {specialBenefits.length ? (
        <section
          className="benefit-deadline-group"
          aria-label="Renewals and purchases"
          data-keyboard-region
        >
          <h3>Renewals & purchases</h3>
          {specialBenefits.map((benefit) => (
            <BenefitRow
              key={benefit.id}
              benefit={benefit}
              externalLogosEnabled={externalLogosEnabled}
              compact={compact}
            />
          ))}
        </section>
      ) : null}
      {!benefits.length ? <EmptyState>No benefits selected in Settings.</EmptyState> : null}
    </>
  )
}

/**
 * The card's year in one line: your posted credits set against the annual fee, which Rust counts
 * from the last fee charge. Past break-even, the meter is full and the line says by how much.
 * Credits on purchases for someone else used the allowance but saved them, so they're named apart.
 */
export function FeeRecovery({
  fee,
}: {
  fee: NonNullable<NonNullable<Insights['benefits']>['fee']>
}) {
  const past = fee.recovered >= fee.annualFee
  const gap = fee.gap
  const money = formatProseCurrency
  return (
    <section className="benefit-fee-recovery" aria-label="Annual fee recovered">
      <p>
        <strong className="sensitive-value">{money(fee.recovered)}</strong> of the{' '}
        <span className="sensitive-value">{money(fee.annualFee)}</span> fee back
        <small className="sensitive-value">
          {past
            ? gap
              ? ` · ${money(gap)} past break-even`
              : ' · break-even'
            : ` · ${money(gap)} to go`}
          {fee.forOthers ? ` · ${money(fee.forOthers)} for others` : ''}
        </small>
      </p>
      <progress
        className="benefit-progress benefit-fee-meter"
        max={fee.annualFee}
        value={Math.min(fee.annualFee, Math.max(0, fee.recovered))}
        aria-label="Annual fee recovered in credits"
      />
      {/* The feed doesn't report the card's anniversary, so a year without a fee charge says so. */}
      <small className="benefit-fee-since">
        {formatDay(fee.since)} – {formatDay(fee.through)}
        {fee.basis === 'fee-charge' ? '' : ' · no fee charge saved'}
        {/* Only credits the periods already plan: waiting purchases and expected charges. */}
        {fee.expected > 0 ? (
          <>
            {' · '}
            <span className="sensitive-value">{money(fee.expected)}</span> more estimated
          </>
        ) : null}
      </small>
    </section>
  )
}

/**
 * A card's benefits at a glance on Spending: the credits to use soonest, as derived by the native
 * benefit rules. The fee, what was missed, and the full year read on Benefits.
 */
export function PlatinumBenefits({
  accountId,
}: {
  /** Another tracked card; omitted for the saved spending card. */
  accountId?: string
}) {
  const data = useFinance().data
  const settings = financeSettings(data)
  const insights = data?.insights
  const externalLogosEnabled = getExternalLogosEnabled()
  const card = accountId
    ? insights?.otherCardBenefits?.find((other) => other.accountId === accountId)
    : insights?.benefits
  const benefits = (card?.windows ?? []).filter(({ id }) => !settings.hiddenBenefitIds.includes(id))
  // It leads with what would be lost soonest, not with the rule file's order.
  const useFirst = benefitsToUseFirst(benefits, card?.useFirst ?? [])
  const review = accountId ? [] : benefitReviewItems(card, settings.hiddenBenefitIds)
  return (
    <Card className="spending-benefits-card">
      <SectionHeading
        title={
          <Link
            to="/analytics"
            search={{ chart: 'amex-credits' }}
            className="section-heading-link"
            data-command="navigation.report.amex-credits"
          >
            {accountId ? `${card?.productName ?? 'Card'} benefits` : 'Benefits'}{' '}
            <ChevronRight size={14} aria-hidden="true" />
          </Link>
        }
      />
      {review.length ? (
        <p className="credit-summary-note">
          <Link
            to="/analytics"
            search={{ chart: 'amex-credits', review: true }}
            data-command="navigation.report.amex-credits"
          >
            {review.length} {review.length === 1 ? 'match' : 'matches'} to review
          </Link>
        </p>
      ) : null}
      {useFirst.length || !benefits.length ? (
        // The most urgent first: grouped by when each resets, soonest deadline leading.
        <BenefitGroups
          benefits={useFirst.slice(0, 3)}
          externalLogosEnabled={externalLogosEnabled}
          compact
        />
      ) : (
        <EmptyState>Every credit in its current window is used.</EmptyState>
      )}
    </Card>
  )
}
