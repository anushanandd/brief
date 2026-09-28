import { Popover } from '@base-ui/react/popover'
import { Link } from '@tanstack/react-router'
import { useEffect, useState, type ReactNode } from 'react'

import { formatCurrency, formatUpdatedAt, formatActivityName } from '../lib/format'
import { brandLogoUrl, brandMarkLabel, getExternalLogosEnabled } from '../lib/logos'
import type { Transaction } from '../lib/schema'
import {
  buildPlatinumBenefitTracker,
  formatActivityDate,
  getPlatinumBenefitActivity,
} from '../lib/spending'
import { getHiddenPlatinumBenefitIds } from '../lib/spending-preferences'
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
import { Button, Card, EmptyState, SectionHeading } from './ui'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

type Benefit = ReturnType<typeof buildPlatinumBenefitTracker>[number]
type BenefitActivity = ReturnType<typeof getPlatinumBenefitActivity>
type BenefitEvidenceItem = BenefitActivity[number]

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

function BenefitMark({
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
  const unavailable = benefit.remainingAmount === null && benefit.estimatedCreditAmount === null
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

function BenefitInfo({ title, children }: { title: string; children: ReactNode }) {
  return (
    <Popover.Root>
      <Popover.Trigger
        aria-label={`About ${title}`}
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
            {(item.kind !== 'credit' || item.pending) && (labels[item.kind] || item.pending) ? (
              <small>
                {[labels[item.kind], item.pending ? 'Pending' : ''].filter(Boolean).join(' · ')}
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
  const external = benefit.status === 'external'
  const complete = benefit.remainingAmount === 0 || benefit.estimatedCreditAmount === benefit.cap
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
  const status = external
    ? benefit.estimatedCreditAmount
      ? `Estimated ${formatCurrency(benefit.estimatedCreditAmount)} Uber Cash used`
      : 'No Uber purchase detected'
    : benefit.snapshotBeforeWindow && benefit.cadence !== 'renewal'
      ? 'No saved activity for this period'
      : benefit.creditedAmount !== 0
        ? `${formatCurrency(benefit.creditedAmount)} ${benefit.creditedAmount < 0 ? 'net reversal' : 'credit detected'}`
        : hasPostedCredits
          ? `${formatCurrency(0)} net credit`
          : benefit.status === 'matched'
            ? 'Purchase found · awaiting credit'
            : 'No credit detected'

  return (
    <article className="spending-benefit-row" aria-labelledby={`benefit-${benefit.id}`}>
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
            {benefit.cadence === 'renewal' ? (
              <p>
                Renewal checks use the last posted credit, not the application date. Cardholder
                identity and complete history may be unavailable.
              </p>
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
        <span className={complete ? 'benefit-detail-status positive' : 'benefit-detail-status'}>
          {benefit.remainingAmount === null
            ? status
            : `${formatCurrency(benefit.creditedAmount)} of ${formatCurrency(benefit.cap)} detected`}
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
        {external ? (
          <small>
            {benefit.estimatedCreditAmount
              ? 'Estimated from Uber purchase'
              : 'No estimated use this month'}
          </small>
        ) : benefit.cadence === 'renewal' ? (
          <small>
            {benefit.renewalCheck
              ? `Approx. renewal check ${benefit.renewalCheck}`
              : 'Renewal date unknown'}
          </small>
        ) : benefit.periodUncertain ? (
          <small>Period uncertain · remaining unknown</small>
        ) : complete ? (
          <small>Full credit detected</small>
        ) : benefit.remainingAmount !== null ? (
          <small className="benefit-remaining">
            Estimated remaining {formatCurrency(benefit.remainingAmount)}
          </small>
        ) : benefit.id === 'walmart-plus' ? (
          <small>Monthly membership reimbursement</small>
        ) : benefit.cadence === 'purchase' ? (
          <small>Credits posted this calendar year</small>
        ) : (
          <small>{benefit.windowLabel}</small>
        )}
      </div>
      {visibleSupportingActivity.length && !external ? (
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
        const group = calendarBenefits.filter(({ windowEnd }) => windowEnd === deadline)
        const first = group[0]
        return (
          <section
            className="benefit-deadline-group"
            key={deadline}
            aria-label={`Benefits ending ${first.reset}`}
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
        <section className="benefit-deadline-group" aria-label="Renewals and purchases">
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

export function PlatinumBenefits({
  transactions,
  snapshotIso,
  preview = false,
}: {
  transactions: Transaction[]
  snapshotIso: string
  preview?: boolean
}) {
  const [now, setNow] = useState(() => new Date().toISOString())
  useEffect(() => {
    const update = () => setNow(new Date().toISOString())
    const timer = window.setInterval(update, 60_000)
    window.addEventListener('focus', update)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener('focus', update)
    }
  }, [])
  const hidden = getHiddenPlatinumBenefitIds()
  const externalLogosEnabled = getExternalLogosEnabled()
  const benefits = buildPlatinumBenefitTracker(transactions, now, snapshotIso).filter(
    ({ id }) => !hidden.includes(id),
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
            >
              Platinum benefits <ChevronRight size={16} aria-hidden="true" />
            </Link>
          }
        />
        <BenefitGroups
          benefits={benefits.slice(0, 3)}
          externalLogosEnabled={externalLogosEnabled}
          compact
        />
      </Card>
    )

  return (
    <Card className="platinum-current-card">
      <SectionHeading
        title="Current benefit windows"
        action={
          <div className="benefit-heading-actions">
            <BenefitInfo title="benefit tracking">
              <p>
                Enrollment and complete usage are unavailable. Credits near a reset or involving
                reversals have no remaining estimate. Allow up to eight weeks for credits, or 90
                days for hotels.
              </p>
              <p>
                Totals use posted credits net of reversals on the saved spending account, including
                hidden and retired benefits, excluding Uber Cash. Most allowances are shared across
                cards on the Amex account.
              </p>
              <p>Saved {formatUpdatedAt(snapshotIso)} · Terms reviewed Sep 20, 2026.</p>
            </BenefitInfo>
            <Link
              className="button-base button-ghost button-compact"
              to="/settings"
              hash="platinum-benefits"
              aria-label="Manage benefits"
            >
              <Settings size={16} aria-hidden="true" />
            </Link>
          </div>
        }
      />
      <p className="benefit-current-scope">As of today · Saved {formatUpdatedAt(snapshotIso)}</p>
      <BenefitGroups benefits={benefits} externalLogosEnabled={externalLogosEnabled} />
      <p className="benefit-preview-note">
        Estimates only. Enrollment and complete usage are unavailable.
      </p>
    </Card>
  )
}
