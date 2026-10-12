import { Popover } from '@base-ui/react/popover'
import { Link } from '@tanstack/react-router'
import { useMemo, type ReactNode } from 'react'

import { useFinance } from '../hooks/use-finance'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import {
  benefitNote,
  benefitStatus,
  benefitWarning,
  creditLagNote,
  creditSummaryNote,
  purchaseCreditLabel,
} from '../lib/benefit-copy'
import { financeSettings } from '../lib/finance-settings'
import {
  formatActivityName,
  formatCurrency,
  formatCurrencyRange,
  formatUpdatedAt,
  formatDay,
} from '../lib/format'
import { brandLogoUrl, brandMarkLabel, getExternalLogosEnabled } from '../lib/logos'
import type { BenefitActivity, BenefitWindow, Insights } from '../lib/schema'
import {
  benefitsToUseFirst,
  benefitWindowSummary,
  formatActivityDate,
  type PlatinumCreditSummary,
} from '../lib/spending'
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
  Settings,
  X,
  type IconComponent,
} from './icons'
import { SettingsLink } from './settings-dialog'
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
            <time dateTime={item.date}>{formatActivityDate(item.date, item.date)}</time>
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
    ? `Last detected credit: ${formatCurrency(benefit.lastCredit.amount)} on ${formatActivityDate(benefit.lastCredit.date, benefit.lastCredit.date)}.`
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
              <p>{creditLagNote(benefit)}</p>
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

function BenefitGroups({
  benefits,
  externalLogosEnabled,
  compact = false,
}: {
  benefits: Benefit[]
  externalLogosEnabled: boolean
  compact?: boolean
}) {
  const calendarBenefits = benefits.filter(
    ({ cadence }) => cadence !== 'renewal' && cadence !== 'purchase',
  )
  const specialBenefits = benefits.filter(
    ({ cadence }) => cadence === 'renewal' || cadence === 'purchase',
  )
  const deadlines = [...new Set(calendarBenefits.map(({ windowEnd }) => windowEnd))]

  return (
    <>
      {deadlines.map((deadline) => {
        // Within a deadline, the most allowance left comes first: it is the most to lose.
        const group = calendarBenefits
          .filter(({ windowEnd }) => windowEnd === deadline)
          .toSorted((a, b) => (b.remainingRange?.high ?? 0) - (a.remainingRange?.high ?? 0))
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

function CreditSummary({
  benefits,
  credits,
  fee,
}: {
  benefits: Benefit[]
  credits?: PlatinumCreditSummary
  fee?: NonNullable<Insights['benefits']>['fee']
}) {
  const summary = benefitWindowSummary(benefits)
  if (!benefits.length) return null
  const missed = credits && credits.missed.high > 0 ? credits.missed : null
  const note = credits ? creditSummaryNote(credits) : ''
  return (
    <>
      <dl className="credit-summary">
        <div>
          <dt>Captured</dt>
          <dd className="sensitive-value">{formatCurrency(summary.captured)}</dd>
        </div>
        <div>
          <dt>Left</dt>
          <dd className="sensitive-value">{formatCurrencyRange(summary.remaining)}</dd>
        </div>
        <div>
          <dt>{missed ? 'Missed' : 'Next reset'}</dt>
          <dd className={missed ? 'sensitive-value negative' : undefined}>
            {missed
              ? formatCurrencyRange(missed)
              : summary.nextReset
                ? summary.nextReset.label
                : '—'}
          </dd>
        </div>
      </dl>
      {note ? <p className="credit-summary-note sensitive-value">{note}</p> : null}
      {fee ? (
        <p className="credit-summary-note">
          <span className="sensitive-value">
            {formatCurrency(fee.recovered)} of {formatCurrency(fee.annualFee)}
          </span>{' '}
          annual fee recovered in posted credits since {formatDay(fee.since)}
          {fee.basis === 'calendar-year' ? ' (calendar year; no fee charge in saved activity)' : ''}
          {fee.forOthers ? (
            <>
              {' · '}
              <span className="sensitive-value">{formatCurrency(fee.forOthers)}</span> more used for
              others
            </>
          ) : null}
        </p>
      ) : null}
    </>
  )
}

/**
 * The card's year in one line: your posted credits set against the annual fee, which Rust counts
 * from the last fee charge. Past break-even, the meter is full and the line says by how much.
 * Credits on purchases for someone else used the allowance but saved them, so they're named apart.
 */
function FeeRecovery({ fee }: { fee: NonNullable<NonNullable<Insights['benefits']>['fee']> }) {
  const past = fee.recovered >= fee.annualFee
  const gap = Math.abs(Math.round((fee.recovered - fee.annualFee) * 100)) / 100
  return (
    <section className="benefit-fee-recovery" aria-label="Annual fee recovered">
      <p>
        <strong className="sensitive-value">{formatCurrency(fee.recovered)}</strong> of the{' '}
        <span className="sensitive-value">{formatCurrency(fee.annualFee)}</span> fee back in credits
        <small className="sensitive-value">
          {past
            ? gap
              ? ` · ${formatCurrency(gap)} past break-even`
              : ' · at break-even'
            : ` · ${formatCurrency(gap)} to break even`}
          {fee.forOthers ? ` · ${formatCurrency(fee.forOthers)} more used for others` : ''}
        </small>
      </p>
      <progress
        className="benefit-progress benefit-fee-meter"
        max={fee.annualFee}
        value={Math.min(fee.annualFee, Math.max(0, fee.recovered))}
        aria-label="Annual fee recovered in credits"
      />
    </section>
  )
}

/** Current windows for the saved spending card, as derived by the native benefit rules. */
export function PlatinumBenefits({
  preview = false,
  credits,
  accountId,
}: {
  preview?: boolean
  /** Credits and unused allowance for windows that ended in the selected spending period. */
  credits?: PlatinumCreditSummary
  /** Another tracked card; omitted for the saved spending card. */
  accountId?: string
}) {
  const data = useFinance().data
  const settings = financeSettings(data)
  const insights = data?.insights
  const snapshotIso = data?.updatedAt ?? ''
  const externalLogosEnabled = getExternalLogosEnabled()
  const card = accountId
    ? insights?.otherCardBenefits?.find((other) => other.accountId === accountId)
    : insights?.benefits
  const benefits = (card?.windows ?? []).filter(({ id }) => !settings.hiddenBenefitIds.includes(id))
  // The preview leads with what would be lost soonest, not with the rule file's order.
  const useFirst = benefitsToUseFirst(benefits, card?.useFirst ?? [])
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        preview
          ? []
          : [
              {
                id: 'benefits.about',
                title: 'About benefit tracking',
                search: 'benefits credits how tracked posted reversal terms',
                group: 'Here',
                scope: 'page',
                run: () =>
                  document.querySelector<HTMLElement>('[data-command="benefits.about"]')?.click(),
              },
            ],
      [preview],
    ),
  )

  if (preview)
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
              {accountId ? `${card?.productName ?? 'Card'} benefits` : 'Platinum benefits'}{' '}
              <ChevronRight size={14} aria-hidden="true" />
            </Link>
          }
        />
        <CreditSummary benefits={benefits} credits={credits} fee={card?.fee} />
        {useFirst.length || !benefits.length ? (
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

  return (
    <Card className="platinum-current-card">
      <SectionHeading
        title="Current benefit windows"
        action={
          <div className="benefit-heading-actions">
            <BenefitInfo title="benefit tracking" command="benefits.about">
              <p>
                Estimated from posted credits, net of reversals. A credit counts in its purchase’s
                period; an unmatched one shows as a range.
              </p>
              <p>
                Credits post within 8 weeks (hotels 90 days). Uber Cash spent in the app never
                shows.
              </p>
              <p>
                Saved {formatUpdatedAt(snapshotIso)} · Terms reviewed{' '}
                {insights?.benefitTermsReviewed ?? 'unavailable'}.
              </p>
            </BenefitInfo>
            <SettingsLink
              className="button-base button-ghost button-compact"
              target="platinum-benefits"
            >
              <Settings size={16} aria-hidden="true" />
              Manage benefits
            </SettingsLink>
          </div>
        }
      />
      <p className="benefit-current-scope">As of today · Saved {formatUpdatedAt(snapshotIso)}</p>
      {card?.fee ? <FeeRecovery fee={card.fee} /> : null}
      <BenefitGroups benefits={benefits} externalLogosEnabled={externalLogosEnabled} />
    </Card>
  )
}
