import { Link } from '@tanstack/react-router'
import { useState } from 'react'
import { toast } from 'sonner'

import { useFinanceEdits } from '../hooks/use-finance-edits'
import {
  activityLocationText,
  activityMethod,
  activityPlace,
  type ActivityItem,
} from '../lib/activity'
import {
  countOf,
  displayCase,
  formatActivityName,
  formatCurrency,
  formatProseCurrency,
  valueTone,
} from '../lib/format'
import { confidenceLabel, needView } from '../lib/needs'
import { placeMapsUrl, placeSource } from '../lib/places'
import type { BenefitActivity, FinanceSnapshot, Transaction } from '../lib/schema'
import { formatActivityDate } from '../lib/spending'
import { mapsUrl, type transactionDetail } from '../lib/transaction-detail'
import { PlaceMark } from './activity-method-mark'
import { ExternalLink } from './external-link'
import {
  AlertCircle,
  ArrowUpRight,
  Check,
  Globe2,
  MapPinned,
  ShieldCheck,
  Unlink,
  X,
} from './icons'
import { Keys } from './keys'
import { AnimatedCurrency, Button } from './ui'

// The inspector's sections, one component each, so `TransactionDetails` reads as their order.

export type Detail = NonNullable<ReturnType<typeof transactionDetail>>

/** The amount, when it was bought and posted, and what Rust flagged about it. */
export function TransactionHero({
  activity,
  detail,
  posted,
  occurred,
  atTime,
  disagree,
  onClose,
}: {
  activity: ActivityItem
  detail: Detail | undefined
  posted: string
  occurred: string | null | undefined
  atTime: string
  /** Some details' sources disagree. */
  disagree: boolean
  onClose: () => void
}) {
  return (
    <section className="transaction-details-hero" aria-label="Amount">
      <AnimatedCurrency
        className={`hero-number sensitive-value ${valueTone(activity.amount)}`}
        value={activity.amount}
      />
      <p>
        {occurred && occurred !== posted ? (
          <>
            Purchased {formatActivityDate(occurred)}
            {atTime}
            <span aria-hidden="true"> · </span>
            posted {formatActivityDate(posted)}
          </>
        ) : (
          `${formatActivityDate(posted)}${atTime}`
        )}
        {detail?.duplicate ? (
          <span className="transaction-details-tag is-warning">
            <AlertCircle size={12} aria-hidden="true" />
            Possible duplicate
          </span>
        ) : null}
        {detail?.unusual ? (
          <span className="transaction-details-tag is-warning">
            <AlertCircle size={12} aria-hidden="true" />
            Unusual amount
          </span>
        ) : null}
        {disagree ? (
          <span className="transaction-details-tag is-warning">
            <AlertCircle size={12} aria-hidden="true" />
            Sources disagree
          </span>
        ) : null}
        {detail?.priceChange ? (
          <span className="transaction-details-tag is-warning">
            Was{' '}
            <span className="sensitive-value">
              {formatCurrency(Math.abs(detail.priceChange.previous))}
            </span>
          </span>
        ) : null}
        {detail?.benefits.map((benefit) => (
          // Opens the benefit's year on Benefits, where its periods and evidence live.
          <Link
            className="transaction-details-tag is-positive"
            key={`${benefit.benefitId}:${benefit.kind}`}
            to="/analytics"
            search={{ chart: 'card-credits', benefit: benefit.benefitId }}
            data-command="navigation.report.card-credits"
            onClick={onClose}
          >
            {benefit.kind === 'credit'
              ? `${benefit.benefitName} credit`
              : `Counts toward ${benefit.benefitName}`}
            <ArrowUpRight size={12} aria-hidden="true" />
          </Link>
        ))}
      </p>
    </section>
  )
}

/**
 * A second look Rust suggests (a possible duplicate, an unusual amount, sources that
 * disagree, or an inferred need with how sure it is and the transactions it read) with the
 * way to mark it reviewed, which dismisses it.
 */
export function ReviewFlag({
  activity,
  data,
  detail,
  transaction,
  conflictText,
  onReviewed,
}: {
  activity: ActivityItem
  data: FinanceSnapshot
  detail: Detail | undefined
  transaction: Transaction | undefined
  /** What each disagreeing source says, when sources disagree. */
  conflictText: string | undefined
  onReviewed?: (id: string) => void
}) {
  const { annotate } = useFinanceEdits()
  const [dismissing, setDismissing] = useState(false)
  const [reviewFailed, setReviewFailed] = useState(false)
  const inferred = detail?.inferred ? needView(detail.inferred, data) : undefined
  if (!detail?.duplicate && !detail?.unusual && !inferred && conflictText === undefined) return null
  // How sure Rust is, and how many transactions it read to say so.
  const basis =
    inferred?.confidence != null
      ? ` ${confidenceLabel(inferred.confidence)}, from ${countOf(inferred.evidence?.length ?? 0, 'transaction')}.`
      : ''
  const markReviewed = () => {
    if (!transaction || dismissing) return
    setDismissing(true)
    setReviewFailed(false)
    void annotate({ [transaction.id]: { reviewed: true } })
      .then(() => onReviewed?.(activity.id))
      .catch(() => setReviewFailed(true))
      .finally(() => setDismissing(false))
  }
  return (
    <div className="transaction-details-flag" role="note">
      <p>
        {detail?.duplicate ? (
          'Same amount, same day. If you paid once, dispute one with the issuer.'
        ) : detail?.unusual ? (
          <>
            Usually{' '}
            <span className="sensitive-value">
              {formatProseCurrency(Math.abs(detail.unusual.usual))}
            </span>{' '}
            here.{basis}
          </>
        ) : inferred ? (
          <>
            {inferred.title}
            {inferred.detail ? (
              <>
                {' · '}
                <span className="sensitive-value">{inferred.detail}</span>
              </>
            ) : null}
            .{basis}
          </>
        ) : (
          <>{conflictText} If that’s right, mark it reviewed.</>
        )}
      </p>
      <Button
        icon={Check}
        size="compact"
        disabled={dismissing}
        aria-busy={dismissing || undefined}
        aria-keyshortcuts="R"
        data-inspector-key="reviewed"
        onClick={markReviewed}
      >
        {detail?.duplicate ? 'Not a duplicate' : 'Looks right'}
      </Button>
      {reviewFailed ? (
        <p className="transaction-details-error" role="alert">
          Couldn’t save. Try again.
        </p>
      ) : null}
    </div>
  )
}

/** Overture's category code in words: `sporting_goods_store` reads "Sporting goods store". */
function categoryLabel(code: string) {
  const words = code.replaceAll('_', ' ')
  return words.charAt(0).toUpperCase() + words.slice(1)
}

/**
 * Where it happened, with links to the place's map and website, then how it was paid beside
 * what Wallet saw at the terminal. Rows of the inspector's facts list.
 */
export function WhereFacts({
  activity,
  transaction,
}: {
  activity: ActivityItem
  transaction: Transaction | undefined
}) {
  const place = activityPlace(activity)
  const address = activityLocationText(activity.location)
  const method = activityMethod(activity)
  const wallet = transaction?.wallet
  const merchant = formatActivityName(activity.title)
  const coordinates = transaction?.location
  // The matched business leads, then the resolved place; the bank's own location stays one
  // click away, in Bank details.
  const resolved = transaction?.place ?? activity.place
  const business = transaction?.business
  const street =
    business?.address ??
    resolved?.address ??
    (resolved?.precision === 'exact' ? transaction?.location?.address : undefined)
  const remote = transaction?.presence === 'remote'
  const billedFrom = remote
    ? [
        transaction?.issuer?.city ?? transaction?.location?.city,
        transaction?.issuer?.region ?? transaction?.location?.region,
      ]
        .filter(Boolean)
        .join(', ')
    : ''
  const businessFacts = [
    business?.category ? categoryLabel(business.category) : '',
    business?.phone ?? wallet?.phone,
    business?.closed ? 'Listed as closed' : '',
  ].filter(Boolean)
  const website = business?.website ?? transaction?.website
  const paid = transaction
    ? [
        wallet?.terminalAmount != null
          ? `Approved for ${formatCurrency(Math.abs(wallet.terminalAmount))} at the terminal`
          : '',
        wallet?.reward && wallet.reward.amount > 0
          ? `Earned ${formatCurrency(wallet.reward.amount)} Daily Cash · ${wallet.reward.percent}%`
          : '',
      ].filter(Boolean)
    : []
  // Google Maps opens the business's card from its name and street address, when known.
  const mapsHref = resolved
    ? placeMapsUrl(
        business?.name ?? merchant,
        business
          ? { ...resolved, lat: business.lat, lon: business.lon, precision: 'exact' }
          : resolved,
        street,
      )
    : !remote && (address || coordinates?.lat != null)
      ? mapsUrl(address || merchant, coordinates)
      : undefined
  return (
    <>
      <div>
        <dt>Where</dt>
        <dd>
          <span className="transaction-details-value">
            {place.kind === 'none' ? null : <PlaceMark kind={place.kind} />}
            <span>
              {place.kind === 'none' ? 'Not reported by the bank' : place.primary}
              {resolved ? (
                <small>
                  {street ? `${displayCase(street)} · ` : ''}
                  {business ? 'From Overture Maps' : placeSource(resolved)}
                </small>
              ) : billedFrom ? (
                <small>Billed from {displayCase(billedFrom)}</small>
              ) : address && address !== place.primary && !remote ? (
                <small>{address}</small>
              ) : null}
              {businessFacts.length ? <small>{businessFacts.join(' · ')}</small> : null}
              {!transaction && place.kind !== 'place' && method && method !== place.primary ? (
                <small>{method}</small>
              ) : null}
            </span>
          </span>
          {mapsHref || website ? (
            <span className="transaction-details-links">
              {mapsHref ? (
                <ExternalLink className="transaction-details-link" href={mapsHref}>
                  <MapPinned size={12} aria-hidden="true" />
                  Open in Google Maps
                </ExternalLink>
              ) : null}
              {website ? (
                <ExternalLink
                  className="transaction-details-link"
                  href={/^https?:\/\//.test(website) ? website : `https://${website}`}
                >
                  <Globe2 size={12} aria-hidden="true" />
                  {website.replace(/^https?:\/\/(www\.)?/, '').replace(/\/$/, '')}
                </ExternalLink>
              ) : null}
            </span>
          ) : null}
        </dd>
      </div>
      {transaction && (method || paid.length) ? (
        <div>
          <dt>Paid</dt>
          <dd>
            {method || 'With the card'}
            {paid.map((line) => (
              <small key={line} className="sensitive-value">
                {line}
              </small>
            ))}
          </dd>
        </div>
      ) : null}
    </>
  )
}

/** Who recorded it besides the bank, and what each detail rests on. A facts list row. */
export function VerifiedFact({
  activity,
  sources,
  quality,
}: {
  activity: ActivityItem
  /** Wallet, a statement, or a pay statement, as they read in a sentence. */
  sources: string[]
  quality: { label: string; value: string; conflict?: boolean }[]
}) {
  return (
    <div>
      <dt>Verified</dt>
      <dd>
        <span className="transaction-details-value">
          {sources.length ? (
            <ShieldCheck size={14} aria-hidden="true" className="positive" />
          ) : null}
          <span>
            {sources.length
              ? `${activity.pending ? 'Pending' : 'Posted'} and confirmed by ${sources.join(' and ')}`
              : activity.pending
                ? 'Pending with the bank'
                : 'Reported by the bank'}
          </span>
        </span>
        {quality.length ? (
          <details className="transaction-details-why">
            <summary>How Brief knows</summary>
            <dl className="transaction-details-facts">
              {quality.map(({ label, value, conflict }) => (
                <div key={label}>
                  <dt>{label}</dt>
                  <dd className={conflict ? 'is-warning' : undefined}>{value}</dd>
                </div>
              ))}
            </dl>
          </details>
        ) : null}
      </dd>
    </div>
  )
}

/** The rest of the activity at the same merchant, with the next expected charge first. */
export function MerchantHistory({
  activity,
  detail,
  merchant,
  posted,
  titleId,
}: {
  activity: ActivityItem
  detail: Detail
  merchant: string
  posted: string
  titleId: string
}) {
  return (
    <section className="transaction-details-history" aria-labelledby={`${titleId}-history`}>
      <h3 id={`${titleId}-history`}>
        At {merchant}
        <span>{detail.historyCount ? `${detail.historyCount + 1} visits` : 'First time here'}</span>
      </h3>
      <ul>
        {detail.expected ? (
          <li className="is-expected">
            <span>{formatActivityDate(detail.expected.date)}</span>
            <small>Expected · {detail.expected.frequency.toLowerCase()}</small>
            <strong className="sensitive-value">
              {detail.expected.amountHigh - detail.expected.amountLow >
              Math.max(5, Math.abs(detail.expected.amount) * 0.1)
                ? '≈ '
                : ''}
              {formatCurrency(detail.expected.amount)}
            </strong>
          </li>
        ) : null}
        <li className="is-current">
          <span>{formatActivityDate(posted)}</span>
          <small>This one</small>
          <strong className={`sensitive-value ${valueTone(activity.amount)}`}>
            {formatCurrency(activity.amount)}
          </strong>
        </li>
        {detail.history.map((item) => (
          <li key={item.id}>
            <span>{formatActivityDate(item.postedOn ?? item.date)}</span>
            <small>
              {[
                item.pending ? 'Pending' : '',
                item.account !== activity.account ? item.account : '',
              ]
                .filter(Boolean)
                .join(' · ')}
            </small>
            <strong className={`sensitive-value ${valueTone(item.amount)}`}>
              {formatCurrency(item.amount)}
            </strong>
          </li>
        ))}
      </ul>
    </section>
  )
}

/** The inspector's keys, as its footer lists them. */
export function InspectorKeysFooter({ detail }: { detail: Detail | undefined }) {
  return (
    <footer className="command-footer transaction-details-keys" aria-label="Keyboard">
      <span className="command-footer-hint">
        Next <Keys label="J" /> <Keys label="K" />
      </span>
      {detail?.duplicate || detail?.unusual || detail?.inferred ? (
        <span className="command-footer-hint">
          Reviewed <Keys label="R" />
        </span>
      ) : null}
      {detail?.categoryOrigin === 'suggested' ? (
        <span className="command-footer-hint">
          Keep <Keys label="A" /> Decline <Keys label="D" />
        </span>
      ) : null}
      {detail && benefitLink(detail.benefits) ? (
        <span className="command-footer-hint">
          Unlink <Keys label="U" />
        </span>
      ) : null}
      {/* The edit rows' first letters: Category, For, Tags, People, Note, Split. */}
      <span className="command-footer-hint">
        Edit <Keys label="C" /> <Keys label="F" /> <Keys label="T" /> <Keys label="P" />{' '}
        <Keys label="N" /> <Keys label="S" />
      </span>
      <span className="command-footer-hint">
        Details <Keys label="⌘I" />
      </span>
      <span className="command-footer-hint">
        Close <Keys label="Esc" />
      </span>
    </footer>
  )
}

/** One source's raw record, under Raw records. */
export function RawRecord({
  title,
  rows,
}: {
  title: string
  rows: { label: string; value: string; raw?: boolean }[]
}) {
  return (
    <section className="transaction-details-record" aria-label={title}>
      <h4>{title}</h4>
      <dl className="transaction-details-facts">
        {rows.map(({ label, value, raw }) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd className={raw ? 'transaction-details-raw' : undefined}>{value}</dd>
          </div>
        ))}
      </dl>
    </section>
  )
}

/** The purchase a benefit match rests on: this one, or the one a credit was paired with. */
function benefitLink(benefits: readonly BenefitActivity[]) {
  for (const match of benefits) {
    if (match.kind === 'purchase') return { match, purchaseId: match.id }
    if (match.kind === 'credit' && match.pairedId) return { match, purchaseId: match.pairedId }
  }
  return undefined
}

/**
 * Which purchase a benefit counts, with a way to undo a wrong match: unlinking takes the
 * purchase out of every benefit, so its credit asks again which purchase earned it.
 */
/**
 * A category Brief chose from your earlier choices: how sure it is, the transactions it
 * followed, and one key to keep it (A, which makes it yours) or decline it (D, which Brief
 * remembers so it never suggests it here again, and counts against it for the merchant).
 */
export function CategorySuggestionFlag({
  data,
  transaction,
}: {
  data: FinanceSnapshot
  transaction: Transaction | undefined
}) {
  const { annotate, updateSettings } = useFinanceEdits()
  const [pending, setPending] = useState(false)
  const [failed, setFailed] = useState(false)
  const suggestion = transaction?.categorySuggestion
  if (!transaction || transaction.categorySource !== 'suggested' || !suggestion) return null
  const { id, category } = transaction
  const run = (edit: () => Promise<unknown>) => {
    if (pending) return
    setPending(true)
    setFailed(false)
    void edit()
      .catch(() => setFailed(true))
      .finally(() => setPending(false))
  }
  // Your answer is kept either way, so Diagnostics can count how often suggestions hold.
  const review = (accepted: boolean) =>
    updateSettings((current) => ({
      ...current,
      categoryReviews: { ...current.categoryReviews, [id]: { category, accepted } },
    }))
  // Keeping makes the category yours, so Reset and undo treat it like any choice.
  const keep = () => run(() => annotate({ [id]: { category } }).then(() => review(true)))
  const reject = () => run(() => review(false))
  const from = suggestion.from.flatMap(
    (fromId) => data.transactions.find((candidate) => candidate.id === fromId) ?? [],
  )
  const shown = from.slice(0, 3)
  return (
    <div className="transaction-details-flag" role="note">
      <p>
        Brief chose {category}, {confidenceLabel(suggestion.confidence)}, from{' '}
        {suggestion.basis === 'merchant'
          ? `your ${countOf(suggestion.from.length, 'earlier choice')} here`
          : `${countOf(suggestion.from.length, 'choice')} at merchants named like this one`}
        {shown.length
          ? `: ${shown
              .map(
                (item) =>
                  `${formatActivityName(item.merchant)} ${formatActivityDate(item.postedOn ?? item.date)}`,
              )
              .join(', ')}${from.length > shown.length ? ', …' : ''}`
          : ''}
        .{transaction.providerCategory ? ` The bank says ${transaction.providerCategory}.` : ''}
      </p>
      <Button
        icon={Check}
        size="compact"
        disabled={pending}
        aria-busy={pending || undefined}
        aria-keyshortcuts="A"
        data-inspector-key="accept"
        onClick={keep}
      >
        Keep
      </Button>
      <Button
        icon={X}
        size="compact"
        variant="ghost"
        disabled={pending}
        aria-busy={pending || undefined}
        aria-keyshortcuts="D"
        data-inspector-key="reject"
        onClick={reject}
      >
        Not {category}
      </Button>
      {failed ? (
        <p className="transaction-details-error" role="alert">
          Couldn’t save. Try again.
        </p>
      ) : null}
    </div>
  )
}

export function BenefitLinkFlag({
  data,
  benefits,
}: {
  data: FinanceSnapshot
  benefits: readonly BenefitActivity[]
}) {
  const { annotate, undo } = useFinanceEdits()
  const [pending, setPending] = useState(false)
  const [failed, setFailed] = useState(false)
  const link = benefitLink(benefits)
  if (!link) return null
  const { match, purchaseId } = link
  const paired = match.pairedId
    ? data.transactions.find(({ id }) => id === match.pairedId)
    : undefined
  const unlink = () => {
    if (pending) return
    setPending(true)
    setFailed(false)
    void annotate({ [purchaseId]: { benefitId: '', benefitConfirmed: false } })
      .then(() =>
        toast.success(`Unlinked from ${match.benefitName}`, {
          action: {
            label: 'Undo',
            onClick: () => void undo().catch(() => toast.error('Couldn’t undo that')),
          },
        }),
      )
      .catch(() => setFailed(true))
      .finally(() => setPending(false))
  }
  return (
    <div className="transaction-details-flag" role="note">
      <p>
        {match.kind === 'credit' && paired ? (
          <>
            Credit for {formatActivityName(paired.merchant)} ·{' '}
            {formatActivityDate(paired.postedOn ?? paired.date)}
          </>
        ) : match.pairedId ? (
          `Earned the ${match.benefitName} credit`
        ) : (
          `Counts toward ${match.benefitName}`
        )}
      </p>
      <Button
        icon={Unlink}
        size="compact"
        variant="ghost"
        disabled={pending}
        aria-busy={pending || undefined}
        aria-keyshortcuts="U"
        data-inspector-key="unlink"
        onClick={unlink}
      >
        Unlink
      </Button>
      {failed ? (
        <p className="transaction-details-error" role="alert">
          Couldn’t save. Try again.
        </p>
      ) : null}
    </div>
  )
}
