import { useQuery } from '@tanstack/react-query'
import { Link } from '@tanstack/react-router'
import { type ReactNode, useState } from 'react'
import { toast } from 'sonner'

import { useFinanceEdits } from '../hooks/use-finance-edits'
import {
  activityLocationText,
  activityMethod,
  activityPlace,
  type ActivityItem,
} from '../lib/activity'
import { withoutCredit } from '../lib/benefit-copy'
import {
  countOf,
  displayCase,
  formatActivityName,
  formatCurrency,
  formatCurrencyIn,
  formatPercent,
  formatProseCurrency,
  valueTone,
} from '../lib/format'
import { confidenceLabel, needView } from '../lib/needs'
import { getObject } from '../lib/objects'
import { placeMapsUrl, placeSource } from '../lib/places'
import type { BenefitActivity, FinanceSnapshot, Trade, Transaction } from '../lib/schema'
import { formatActivityDate, formatListDate } from '../lib/spending'
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
import { RawTree } from './object-details'
import { AnimatedCurrency, Button } from './ui'

// The inspector's sections, one component each, so `TransactionDetails` reads as their order.

export type Detail = NonNullable<ReturnType<typeof transactionDetail>>

/** The amount, when it was bought and posted and how it was paid, and what Rust flagged. */
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
  const method = activity.transfer || activity.kind === 'trade' ? '' : activityMethod(activity)
  return (
    <section className="transaction-details-hero" aria-label="Amount">
      <AnimatedCurrency
        className={`hero-number sensitive-value ${valueTone(activity.amount)}`}
        value={activity.amount}
      />
      <p>
        {/* When and how, as one phrase, so the tags after it wrap as a group. */}
        <span>
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
          {method && method !== 'Other' ? (
            <>
              <span aria-hidden="true"> · </span>
              {method}
            </>
          ) : null}
        </span>
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
              ? `${withoutCredit(benefit.benefitName)} credit`
              : `Counts toward ${withoutCredit(benefit.benefitName)}`}
            <ArrowUpRight size={12} aria-hidden="true" />
          </Link>
        ))}
      </p>
    </section>
  )
}

function reviewFlagShows(detail: Detail | undefined, conflictText: string | undefined) {
  return Boolean(
    detail?.duplicate || detail?.unusual || detail?.inferred || conflictText !== undefined,
  )
}

function suggestionShows(transaction: Transaction | undefined) {
  return transaction?.categorySource === 'suggested' && Boolean(transaction.categorySuggestion)
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
  if (!reviewFlagShows(detail, conflictText)) return null
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
  const foreign = wallet?.foreign
  const reward = wallet?.reward
  // How it was paid reads in the hero; this row holds only what Wallet adds to it.
  const paid = transaction
    ? [
        foreign
          ? `Charged ${formatCurrencyIn(Math.abs(foreign.amount), foreign.currency)}${foreign.rate ? ` at ${foreign.rate} per dollar` : ''}`
          : '',
        wallet?.terminalAmount != null
          ? `Approved for ${formatCurrency(Math.abs(wallet.terminalAmount))} at the terminal`
          : '',
        reward && reward.amount > 0
          ? `Earned ${formatCurrency(reward.amount)} Daily Cash · ${reward.percent}%`
          : reward && reward.amount < 0
            ? `Returned ${formatCurrency(-reward.amount)} Daily Cash`
            : '',
      ].filter(Boolean)
    : []
  // A place leads; online or in store, the hero already says how, so only where it billed from
  // or what's known about the business is left to say.
  const named = place.kind === 'place' || (!transaction && place.kind !== 'none')
  const lead = named ? place.primary : billedFrom ? `Billed from ${displayCase(billedFrom)}` : ''
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
  const where = lead || resolved || businessFacts.length || mapsHref || website
  return (
    <>
      {where ? (
        <div>
          <dt>Where</dt>
          <dd>
            <span className="transaction-details-value">
              {place.kind === 'none' ? null : <PlaceMark kind={place.kind} />}
              <span>
                {lead || null}
                {resolved ? (
                  <small>
                    {street ? `${displayCase(street)} · ` : ''}
                    {business ? 'From Overture Maps' : placeSource(resolved)}
                  </small>
                ) : address && address !== place.primary && !remote && named ? (
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
      ) : null}
      {paid.length ? (
        <div>
          <dt>Paid</dt>
          <dd>
            {paid.map((line) => (
              <span key={line} className="sensitive-value">
                {line}
              </span>
            ))}
          </dd>
        </div>
      ) : null}
    </>
  )
}

/**
 * A trade's facts: the security, linked to its holding, its shares at their price, and what the
 * sale cost and gained, split into short and long term. Rows of the inspector's facts list.
 */
export function TradeFacts({ trade, onClose }: { trade: Trade; onClose: () => void }) {
  const gain = trade.estimatedRealizedGain
  const terms = [
    trade.estimatedShortTermGain != null
      ? `${formatCurrency(trade.estimatedShortTermGain)} short term`
      : '',
    trade.estimatedLongTermGain != null
      ? `${formatCurrency(trade.estimatedLongTermGain)} long term`
      : '',
  ].filter(Boolean)
  return (
    <>
      <div>
        <dt>Security</dt>
        <dd>
          {trade.ticker ? (
            <Link
              className="transaction-details-link"
              to="/holdings/$ticker"
              params={{ ticker: trade.ticker }}
              onClick={onClose}
            >
              {trade.ticker}
              <ArrowUpRight size={12} aria-hidden="true" />
            </Link>
          ) : (
            'Not named'
          )}
          {trade.description ? <small>{trade.description}</small> : null}
        </dd>
      </div>
      {trade.units != null ? (
        <div>
          <dt>Shares</dt>
          <dd className="sensitive-value">
            {trade.price != null
              ? `${trade.units} at ${formatCurrency(trade.price)}`
              : String(trade.units)}
            {trade.splitRatio ? (
              <small>Restated for a {trade.splitRatio}-for-1 split since</small>
            ) : null}
          </dd>
        </div>
      ) : null}
      {trade.realizedCostBasis != null ? (
        <div>
          <dt>Cost basis</dt>
          <dd className="sensitive-value">{formatCurrency(trade.realizedCostBasis)}</dd>
        </div>
      ) : null}
      {gain != null ? (
        <div>
          <dt>Estimated P/L</dt>
          <dd>
            <span className={`sensitive-value ${valueTone(gain)}`}>
              {formatCurrency(gain)}
              {trade.estimatedRealizedGainPct != null
                ? ` · ${formatPercent(trade.estimatedRealizedGainPct)}`
                : ''}
            </span>
            <small className="sensitive-value">
              {[
                ...terms,
                !trade.realizedGainMethod || trade.realizedGainMethod === 'estimated-fifo'
                  ? 'First in, first out'
                  : trade.realizedGainMethod,
              ].join(' · ')}
            </small>
          </dd>
        </div>
      ) : null}
    </>
  )
}

type RecordRows = { label: string; value: string; raw?: boolean }[]

/** One fact as Brief shows it, what each source said of it, and what Brief's choice rests on. */
export type SourceRow = {
  label: string
  shown: string
  says: string[]
  basis?: string
  conflict?: boolean
}

/**
 * Who reported it and who confirmed it, then, opened, what each source said of the facts that
 * matter beside what Brief shows and why, and each source's record: its fields in words, then
 * every field as it was sent. The records load only when opened.
 */
export function SourcesSection({
  pending,
  reportedBy,
  confirmedBy,
  comparison,
  records,
  objectId,
  titleId,
}: {
  pending: boolean
  /** Plaid, SnapTrade, or Apple Wallet for a row only it holds. */
  reportedBy: string
  /** Wallet, a statement, or a pay statement, as they read in a sentence. */
  confirmedBy: string[]
  comparison: SourceRow[]
  /** Each source's fields in words, by its name. */
  records: Array<{ source: string; rows: RecordRows }>
  objectId: string | undefined
  titleId: string
}) {
  const [open, setOpen] = useState(false)
  const sent = useQuery({
    queryKey: ['object', objectId],
    queryFn: () => getObject(objectId ?? ''),
    enabled: open && Boolean(objectId),
  })
  const raw: Record<string, unknown> = sent.data?.sources ?? {}
  const worded = new Map(records.filter(({ rows }) => rows.length).map((r) => [r.source, r.rows]))
  const names = [...new Set([...worded.keys(), ...Object.keys(raw)])]
  return (
    <section className="transaction-details-sources" aria-labelledby={`${titleId}-sources`}>
      <h3 id={`${titleId}-sources`}>Sources</h3>
      <p className="transaction-details-value">
        {confirmedBy.length ? (
          <ShieldCheck size={14} aria-hidden="true" className="positive" />
        ) : null}
        <span>
          {`${pending ? 'Pending · ' : ''}Reported by ${reportedBy}${confirmedBy.length ? `, confirmed by ${confirmedBy.join(' and ')}` : ''}`}
        </span>
      </p>
      {comparison.length || names.length || objectId ? (
        <details
          className="transaction-details-why"
          onToggle={(event) => setOpen(event.currentTarget.open)}
        >
          <summary>{comparison.length ? 'Who said what' : `What ${reportedBy} sent`}</summary>
          <dl className="transaction-details-facts">
            {comparison.map(({ label, shown, says, basis, conflict }) => (
              <div key={label}>
                <dt>{label}</dt>
                <dd>
                  <span className="sensitive-value">{shown}</span>
                  {says.map((line) => (
                    <small key={line} className="sensitive-value">
                      {line}
                    </small>
                  ))}
                  {basis ? (
                    <small className={conflict ? 'is-warning' : undefined}>{basis}</small>
                  ) : null}
                </dd>
              </div>
            ))}
          </dl>
          {names.map((name) => (
            <RawRecord key={name} title={name} rows={worded.get(name) ?? []}>
              {raw[name] ? (
                <details className="transaction-details-why">
                  <summary>Every field from {name}</summary>
                  <RawTree value={raw[name]} />
                </details>
              ) : null}
            </RawRecord>
          ))}
        </details>
      ) : null}
    </section>
  )
}

/** The rest of the activity at the same merchant, with the next expected charge first. */
export function MerchantHistory({
  activity,
  detail,
  merchant,
  posted,
  titleId,
  referenceIso,
  onOpen,
}: {
  activity: ActivityItem
  detail: Detail
  merchant: string
  posted: string
  titleId: string
  /** The snapshot's time, so dates name the year only when it isn't this one. */
  referenceIso: string
  /** Opens an earlier visit, by its transaction id. */
  onOpen: (id: string) => void
}) {
  const day = (date: string) => formatListDate(date, referenceIso)
  return (
    <section className="transaction-details-history" aria-labelledby={`${titleId}-history`}>
      <h3 id={`${titleId}-history`}>
        At {merchant}
        <span>{detail.historyCount ? `${detail.historyCount + 1} visits` : 'First time here'}</span>
      </h3>
      <ul>
        {detail.expected ? (
          <li className="is-expected">
            <span>{day(detail.expected.date)}</span>
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
          <span>{day(posted)}</span>
          <small>This one</small>
          <strong className={`sensitive-value ${valueTone(activity.amount)}`}>
            {formatCurrency(activity.amount)}
          </strong>
        </li>
        {/* An earlier visit opens in place, as its row would in a list. */}
        {detail.history.map((item) => (
          <li key={item.id}>
            <button
              type="button"
              className="list-row transaction-history-row"
              aria-label={`Open ${merchant} on ${day(item.postedOn ?? item.date)}`}
              onClick={() => onOpen(item.id)}
            >
              <span>{day(item.postedOn ?? item.date)}</span>
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
            </button>
          </li>
        ))}
      </ul>
    </section>
  )
}

/**
 * The inspector's keys, as its footer lists them: each only while the control or action it
 * presses exists, read from the same conditions the flags and the dialog use.
 */
export function InspectorKeysFooter({
  detail,
  transaction,
  conflictText,
  canStep,
  hasObject,
}: {
  detail: Detail | undefined
  transaction: Transaction | undefined
  conflictText: string | undefined
  canStep: boolean
  hasObject: boolean
}) {
  return (
    <footer className="command-footer transaction-details-keys" aria-label="Keyboard">
      {canStep ? (
        <span className="command-footer-hint">
          Next <Keys label="J" /> <Keys label="K" />
        </span>
      ) : null}
      {reviewFlagShows(detail, conflictText) ? (
        <span className="command-footer-hint">
          Reviewed <Keys label="R" />
        </span>
      ) : null}
      {suggestionShows(transaction) ? (
        <span className="command-footer-hint">
          Keep <Keys label="A" /> Decline <Keys label="D" />
        </span>
      ) : null}
      {detail && benefitLink(detail.benefits) ? (
        <span className="command-footer-hint">
          Unlink <Keys label="U" />
        </span>
      ) : null}
      {/* The edit rows' first letters: Category, For, Tags, People, Note, Split; B links a
          payback, as in paid Back. */}
      {transaction ? (
        <span className="command-footer-hint">
          Edit <Keys label="C" /> <Keys label="F" /> <Keys label="T" /> <Keys label="P" />{' '}
          <Keys label="N" /> <Keys label="S" /> <Keys label="B" />
        </span>
      ) : null}
      {hasObject ? (
        <span className="command-footer-hint">
          Details <Keys label="⌘I" />
        </span>
      ) : null}
      <span className="command-footer-hint">
        Close <Keys label="Esc" />
      </span>
    </footer>
  )
}

/** One source's record under Sources: its fields in words, then whatever follows them. */
function RawRecord({
  title,
  rows,
  children,
}: {
  title: string
  rows: RecordRows
  children?: ReactNode
}) {
  return (
    <section className="transaction-details-record" aria-label={title}>
      <h4>{title}</h4>
      {rows.length ? (
        <dl className="transaction-details-facts">
          {rows.map(({ label, value, raw }) => (
            <div key={label}>
              <dt>{label}</dt>
              <dd className={raw ? 'transaction-details-raw' : undefined}>{value}</dd>
            </div>
          ))}
        </dl>
      ) : null}
      {children}
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
  if (!transaction || !suggestionShows(transaction) || !suggestion) return null
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
        toast.success(`Unlinked from ${withoutCredit(match.benefitName)}`, {
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
          `Earned the ${withoutCredit(match.benefitName)} credit`
        ) : (
          `Counts toward ${withoutCredit(match.benefitName)}`
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
