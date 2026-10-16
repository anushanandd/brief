import { Link } from '@tanstack/react-router'
import { useEffect, useId, useState } from 'react'
import { toast } from 'sonner'

import { useFinanceEdits } from '../hooks/use-finance-edits'
import {
  activityLocationText,
  activityMethod,
  activityPlace,
  type ActivityItem,
} from '../lib/activity'
import { documentTitle, transactionDocuments } from '../lib/documents'
import {
  displayCase,
  formatActivityName,
  formatCurrency,
  formatProseCurrency,
  valueTone,
} from '../lib/format'
import { shortcutInput } from '../lib/keyboard'
import { placeLabel, placeMapsUrl, placeSource } from '../lib/places'
import type { BenefitActivity, FinanceSnapshot, QualityEvidence, Transaction } from '../lib/schema'
import { formatActivityDate } from '../lib/spending'
import { mapsUrl, transactionDetail } from '../lib/transaction-detail'
import { ActivityMark } from './activity-list'
import { PlaceMark } from './activity-method-mark'
import { CategoryMark } from './category-mark'
import { ExternalLink } from './external-link'
import {
  AlertCircle,
  ArrowUpRight,
  Check,
  FileText,
  MapPinned,
  Plane,
  Repeat2,
  ShieldCheck,
  Unlink,
  X,
} from './icons'
import { Keys } from './keys'
import { escapeClosesSettings } from './settings-dialog'
import { TransactionEdits } from './transaction-edits'
import { AnimatedCurrency, Button } from './ui'
import { PopupContainerContext } from './ui/popup-container'
import { usePresence } from './ui/presence'

/**
 * The inspector's own keys: J and K step through the list; C, T, P, N, and S start an edit; F
 * switches between yours and someone else's; R marks a flagged transaction reviewed; U unlinks
 * a benefit match; O opens a transfer's other side.
 */
export function inspectorKey(key: string) {
  switch (key) {
    case 'j':
    case 'ArrowDown':
      return 'next'
    case 'k':
    case 'ArrowUp':
      return 'previous'
    case 'c':
      return 'category'
    case 't':
      return 'tag'
    case 'p':
      return 'people'
    case 'n':
      return 'note'
    case 's':
      return 'split'
    case 'f':
      return 'other'
    case 'r':
      return 'reviewed'
    case 'u':
      return 'unlink'
    case 'o':
      return 'transfer'
    default:
      return undefined
  }
}

/**
 * One activity in full, as a modal like Settings: the amount, when and where it happened, where
 * its category came from, and the rest of the activity at the same merchant.
 */
export function TransactionDialog({
  activity,
  data,
  onClose,
  onStep,
  onReviewed,
  onOpenActivity,
}: {
  activity: ActivityItem | undefined
  data: FinanceSnapshot
  onClose: () => void
  /** Opens the next (1) or previous (-1) row of the list this was opened from. */
  onStep?: (direction: 1 | -1) => void
  /** A flagged transaction was marked reviewed; a review queue moves on. */
  onReviewed?: (id: string) => void
  /** Shows another activity in place; without it, a link to one opens it in Activity. */
  onOpenActivity?: (id: string) => void
}) {
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  const titleId = useId()
  // The last activity stays in view while the inspector slides back to the edge.
  const shown = usePresence(activity, dialog)
  useEffect(() => {
    if (!dialog || typeof dialog.showModal !== 'function') return
    if (activity && !dialog.open) dialog.showModal()
    else if (!activity && dialog.open) dialog.close()
  }, [activity, dialog])

  // App shortcuts pause while a dialog is open, so the inspector handles its own keys. The
  // listener is on the window so a key still works after stepping replaces the focused content.
  const open = Boolean(activity)
  useEffect(() => {
    if (!open || !dialog) return undefined
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        event.isComposing ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        shortcutInput(event.target)
      )
        return
      const key = inspectorKey(event.key)
      if (!key) return
      event.preventDefault()
      if (key === 'next' || key === 'previous') {
        onStep?.(key === 'next' ? 1 : -1)
        requestAnimationFrame(() =>
          dialog.querySelector<HTMLElement>('.transaction-details-header h2')?.focus(),
        )
      } else dialog.querySelector<HTMLElement>(`[data-inspector-key="${key}"]`)?.click()
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [dialog, onStep, open])

  return (
    <dialog
      ref={setDialog}
      className="transaction-dialog"
      aria-labelledby={titleId}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onKeyDown={(event) => {
        if (event.nativeEvent.isComposing) return
        if (event.key === 'Escape') {
          if (!escapeClosesSettings(event.target)) return
          event.preventDefault()
          onClose()
          return
        }
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      {shown ? (
        <PopupContainerContext.Provider value={dialog}>
          <TransactionDetails
            key={shown.id}
            activity={shown}
            data={data}
            titleId={titleId}
            onClose={onClose}
            onReviewed={onReviewed}
            onOpenActivity={onOpenActivity}
          />
        </PopupContainerContext.Provider>
      ) : null}
    </dialog>
  )
}

function TransactionDetails({
  activity,
  data,
  titleId,
  onClose,
  onReviewed,
  onOpenActivity,
}: {
  activity: ActivityItem
  data: FinanceSnapshot
  titleId: string
  onClose: () => void
  onReviewed?: (id: string) => void
  onOpenActivity?: (id: string) => void
}) {
  const detail = transactionDetail(data, activity.id)
  const transaction = detail?.transaction
  const place = activityPlace(activity)
  const transfer = activity.transfer
  const address = activityLocationText(activity.location)
  const method = activityMethod(activity)
  const posted = transaction?.postedOn ?? activity.date
  const occurred = transaction?.occurredOn
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
  const businessFacts = business
    ? [
        business.category ? categoryLabel(business.category) : '',
        business.phone,
        business.closed ? 'Listed as closed' : '',
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
  const { annotate } = useFinanceEdits()
  const [dismissing, setDismissing] = useState(false)
  const [reviewFailed, setReviewFailed] = useState(false)
  const markReviewed = () => {
    if (!transaction || dismissing) return
    setDismissing(true)
    setReviewFailed(false)
    void annotate({ [transaction.id]: { reviewed: true } })
      .then(() => onReviewed?.(activity.id))
      .catch(() => setReviewFailed(true))
      .finally(() => setDismissing(false))
  }
  const details = transaction ? bankDetails(transaction) : []
  // Documents that state this transaction: a statement lists it, or a pay statement paid it.
  const evidence = transaction ? transactionDocuments(data, transaction.id) : []
  const onStatement = evidence.some(({ kind }) => kind === 'statement')
  const onPayStatement = evidence.some(({ kind }) => kind === 'pay')
  // Details whose sources disagree, until the user marks the transaction reviewed.
  const conflicts =
    transaction?.quality && !transaction.reviewed
      ? evidenceDetails(transaction)
          .filter(({ conflict }) => conflict)
          .map(({ label }) => label)
      : []

  const categoryOrigin =
    detail?.categoryOrigin === 'manual'
      ? 'Set by you'
      : transaction?.providerCategory && transaction.providerCategory !== activity.category
        ? `Bank says ${transaction.providerCategory}`
        : 'From the bank'

  return (
    <article className="transaction-details">
      <header className="transaction-details-header">
        <ActivityMark activity={activity} />
        <div>
          {/* The title takes focus on open, so no control starts out ringed. */}
          <h2 id={titleId} tabIndex={-1} autoFocus>
            {merchant}
          </h2>
          <p>
            {activity.account ?? 'Account unavailable'}
            {activity.pending ? ' · Pending' : ''}
          </p>
        </div>
        <Button
          icon={X}
          size="icon-compact"
          variant="ghost"
          aria-label="Close details"
          onClick={onClose}
        />
      </header>

      <section className="transaction-details-hero" aria-label="Amount">
        <AnimatedCurrency
          className={`hero-number sensitive-value ${valueTone(activity.amount)}`}
          value={activity.amount}
        />
        <p>
          {occurred && occurred !== posted ? (
            <>
              Purchased {formatActivityDate(occurred)}
              <span aria-hidden="true"> · </span>
              posted {formatActivityDate(posted)}
            </>
          ) : (
            formatActivityDate(posted)
          )}
          {detail?.expected ? (
            <span className="transaction-details-tag">
              <Repeat2 size={12} aria-hidden="true" />
              {detail.expected.frequency}
            </span>
          ) : null}
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
          {conflicts.length ? (
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
          {detail?.trip ? (
            <span className="transaction-details-tag">
              <Plane size={12} aria-hidden="true" />
              {detail.trip.destination}
            </span>
          ) : null}
          {onStatement || onPayStatement ? (
            <span className="transaction-details-tag is-positive">
              <ShieldCheck size={12} aria-hidden="true" />
              {onStatement ? 'On the statement' : 'Matches the pay statement'}
            </span>
          ) : null}
          {transaction?.dividendType ? (
            <span className="transaction-details-tag">
              {transaction.dividendType === 'qualified'
                ? 'Qualified dividend'
                : 'Ordinary dividend'}
            </span>
          ) : null}
          {detail?.benefits.map((benefit) => (
            // Opens the benefit's year on Benefits, where its periods and evidence live.
            <Link
              className="transaction-details-tag is-positive"
              key={`${benefit.benefitId}:${benefit.kind}`}
              to="/analytics"
              search={{ chart: 'amex-credits', benefit: benefit.benefitId }}
              data-command="navigation.report.amex-credits"
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

      <div className="transaction-details-body">
        {detail?.duplicate || detail?.unusual || conflicts.length ? (
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
                  here.
                </>
              ) : (
                <>
                  {transaction
                    ? conflicts.map((label) => conflictNote(transaction, label)).join(' ')
                    : null}{' '}
                  If that’s right, mark it reviewed.
                </>
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
        ) : null}
        {detail ? <BenefitLinkFlag data={data} benefits={detail.benefits} /> : null}
        <dl className="transaction-details-facts">
          {transaction ? null : (
            <div>
              <dt>Category</dt>
              <dd>
                <span className="transaction-details-value">
                  <CategoryMark
                    category={activity.category}
                    kind={activity.kind}
                    mark={activity.mark}
                    amount={activity.amount}
                  />
                  <span>
                    {activity.category}
                    {activity.subcategory ? <small>{activity.subcategory}</small> : null}
                  </span>
                </span>
                {transaction ? <small>{categoryOrigin}</small> : null}
              </dd>
            </div>
          )}
          {transfer ? (
            <div>
              <dt>{transfer.direction === 'to' ? 'Moved to' : 'Moved from'}</dt>
              <dd>
                <span className="transaction-details-value">
                  <PlaceMark kind="transfer" />
                  <span>
                    {transfer.account}
                    <small>
                      {transfer.date === posted
                        ? 'Same day'
                        : `${transfer.direction === 'to' ? 'Arrived' : 'Sent'} ${formatActivityDate(transfer.date)}`}
                    </small>
                  </span>
                </span>
                <Link
                  className="transaction-details-link"
                  to="/activities"
                  search={{ open: transfer.id.replace(/^spending:/, '') }}
                  data-command="navigation.open./activities"
                  data-inspector-key="transfer"
                  aria-keyshortcuts="O"
                  onClick={(event) => {
                    if (!onOpenActivity) return onClose()
                    event.preventDefault()
                    onOpenActivity(transfer.id)
                  }}
                >
                  <ArrowUpRight size={12} aria-hidden="true" />
                  Open the other side
                </Link>
              </dd>
            </div>
          ) : (
            <>
              {activity.mark === 'transfer' && transaction && !transaction.pending ? (
                <div>
                  <dt>Other side</dt>
                  <dd>Not matched to another of your accounts</dd>
                </div>
              ) : null}
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
                      {place.kind !== 'place' && method && method !== place.primary ? (
                        <small>{method}</small>
                      ) : null}
                    </span>
                  </span>
                  {mapsHref ? (
                    <ExternalLink className="transaction-details-link" href={mapsHref}>
                      <MapPinned size={12} aria-hidden="true" />
                      Open in Google Maps
                    </ExternalLink>
                  ) : null}
                </dd>
              </div>
            </>
          )}
          {activity.description && activity.description !== activity.title ? (
            <div>
              <dt>Bank text</dt>
              <dd className="transaction-details-raw">{activity.description}</dd>
            </div>
          ) : null}
          {detail?.trip ? (
            <div>
              <dt>Trip</dt>
              <dd>
                {detail.trip.destination}
                <small>
                  {formatActivityDate(detail.trip.start)} – {formatActivityDate(detail.trip.end)} ·{' '}
                  <span className="sensitive-value">
                    {formatProseCurrency(Math.abs(detail.trip.total))}
                  </span>
                </small>
              </dd>
            </div>
          ) : null}
          {evidence.length ? (
            <div>
              <dt>Documents</dt>
              <dd className="transaction-details-documents">
                {evidence.map(({ kind, document }) => (
                  <Link
                    key={document.id}
                    className="transaction-details-link"
                    to="/documents"
                    search={{ year: document.taxYear, document: document.id }}
                    data-command="navigation.open./documents"
                    onClick={onClose}
                  >
                    <FileText size={12} aria-hidden="true" />
                    {documentTitle(document)}
                    <small>
                      {kind === 'statement'
                        ? `Lists it${
                            document.periodStart && document.periodEnd
                              ? ` · ${formatActivityDate(document.periodStart)} – ${formatActivityDate(document.periodEnd)}`
                              : ''
                          }`
                        : `Deposit of its net pay${document.date ? ` · ${formatActivityDate(document.date)}` : ''}`}
                    </small>
                  </Link>
                ))}
              </dd>
            </div>
          ) : null}
          {activity.estimatedRealizedGain != null ? (
            <div>
              <dt>Estimated P/L</dt>
              <dd className={valueTone(activity.estimatedRealizedGain)}>
                {formatCurrency(activity.estimatedRealizedGain)}
              </dd>
            </div>
          ) : null}
        </dl>

        {transaction?.quality ? (
          <details className="transaction-details-more">
            <summary>Evidence</summary>
            <dl className="transaction-details-facts">
              {evidenceDetails(transaction).map(({ label, value, conflict }) => (
                <div key={label}>
                  <dt>{label}</dt>
                  <dd className={conflict ? 'is-warning' : undefined}>{value}</dd>
                </div>
              ))}
            </dl>
          </details>
        ) : null}

        {details.length ? (
          <details className="transaction-details-more">
            <summary>Bank details</summary>
            <dl className="transaction-details-facts">
              {details.map(({ label, value, raw }) => (
                <div key={label}>
                  <dt>{label}</dt>
                  <dd className={raw ? 'transaction-details-raw' : undefined}>{value}</dd>
                </div>
              ))}
            </dl>
          </details>
        ) : null}

        {transaction ? (
          <TransactionEdits
            key={transaction.id}
            transaction={transaction}
            data={data}
            categoryNote={categoryOrigin}
          />
        ) : null}

        {detail ? (
          <section className="transaction-details-history" aria-labelledby={`${titleId}-history`}>
            <h3 id={`${titleId}-history`}>
              At {merchant}
              <span>
                {detail.historyCount ? `${detail.historyCount + 1} visits` : 'First time here'}
              </span>
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
        ) : null}
      </div>
      <footer className="command-footer transaction-details-keys" aria-label="Keyboard">
        <span className="command-footer-hint">
          Next <Keys label="J" /> <Keys label="K" />
        </span>
        {detail?.duplicate || detail?.unusual ? (
          <span className="command-footer-hint">
            Reviewed <Keys label="R" />
          </span>
        ) : null}
        {detail && benefitLink(detail.benefits) ? (
          <span className="command-footer-hint">
            Unlink <Keys label="U" />
          </span>
        ) : null}
        <span className="command-footer-hint">
          Category <Keys label="C" />
        </span>
        <span className="command-footer-hint">
          Tag <Keys label="T" />
        </span>
        <span className="command-footer-hint">
          People <Keys label="P" />
        </span>
        <span className="command-footer-hint">
          Note <Keys label="N" />
        </span>
        <span className="command-footer-hint">
          Split <Keys label="S" />
        </span>
        <span className="command-footer-hint">
          Me or other <Keys label="F" />
        </span>
        <span className="command-footer-hint">
          Close <Keys label="Esc" />
        </span>
      </footer>
    </article>
  )
}

const confidenceLabels: Record<string, string> = {
  VERY_HIGH: 'very high confidence',
  HIGH: 'high confidence',
  MEDIUM: 'medium confidence',
  LOW: 'low confidence',
}
const counterpartyLabels: Record<string, string> = {
  merchant: 'Merchant',
  financial_institution: 'Bank',
  payment_app: 'Payment app',
  payment_terminal: 'Payment terminal',
  marketplace: 'Marketplace',
  income_source: 'Income source',
}

/** What the bank and Plaid reported beyond the essentials, for the Bank details disclosure. */
/** The purchase a benefit match rests on: this one, or the one a credit was paired with. */
function benefitLink(benefits: readonly BenefitActivity[]) {
  const match = benefits.find(
    ({ kind, pairedId }) => kind === 'purchase' || (kind === 'credit' && pairedId),
  )
  if (!match) return undefined
  return { match, purchaseId: match.kind === 'purchase' ? match.id : match.pairedId! }
}

/**
 * Which purchase a benefit counts, with a way to undo a wrong match: unlinking takes the
 * purchase out of every benefit, so its credit asks again which purchase earned it.
 */
function BenefitLinkFlag({
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

/** Overture's category code in words: `sporting_goods_store` reads "Sporting goods store". */
function categoryLabel(code: string) {
  const words = code.replaceAll('_', ' ')
  return words[0].toUpperCase() + words.slice(1)
}

const evidenceLabels: Record<QualityEvidence['basis'], string> = {
  statement: 'Matches the statement',
  statementDiffers: 'The statement shows another amount',
  statementOnly: 'From an imported statement',
  pending: 'Pending; may change when it posts',
  provider: 'Posted by the bank',
  providerConfident: 'Plaid is sure',
  providerUnsure: 'Plaid isn’t sure',
  manual: 'Set by you',
  sourcesAgree: 'Bank text and Plaid agree',
  sourcesDisagree: 'Bank text and Plaid disagree',
  descriptorOnly: 'From the bank text',
  bankOnly: 'From Plaid only',
  unreported: 'Not reported',
  issuer: 'From the card issuer’s records',
  providerContradicted: 'The bank’s records contradict Plaid',
  placeMatched: 'A business here has this name',
}

/**
 * What each detail of a transaction rests on, as Rust graded it. A detail whose sources
 * disagree names what each says, so the user can judge which is right.
 */
export function evidenceDetails(transaction: Transaction) {
  const quality = transaction.quality
  if (!quality) return []
  const plaidPlace = [transaction.location?.city, transaction.location?.region]
    .filter(Boolean)
    .join(', ')
  const rows: Array<{ label: string; evidence: QualityEvidence; value?: string } | undefined> = [
    {
      label: 'Amount',
      evidence: quality.amount,
      value:
        quality.statementAmount == null
          ? undefined
          : `Plaid ${formatCurrency(transaction.amount)}, statement ${formatCurrency(quality.statementAmount)}`,
    },
    quality.merchant && {
      label: 'Merchant',
      evidence: quality.merchant,
      value: transaction.providerMerchant
        ? `Plaid called it ${transaction.providerMerchant}; the bank’s records don’t`
        : undefined,
    },
    { label: 'Category', evidence: quality.category },
    quality.location && {
      label: 'Place',
      evidence: quality.location,
      value:
        quality.location.basis === 'sourcesDisagree'
          ? `Bank text ${placeLabel(transaction.place)}, Plaid ${plaidPlace}`
          : undefined,
    },
  ]
  return rows.flatMap((row) =>
    row
      ? [
          {
            label: row.label,
            value: row.value ?? evidenceLabels[row.evidence.basis],
            conflict: row.evidence.grade === 'conflict',
          },
        ]
      : [],
  )
}

/** What a conflicted detail's sources say, and which one Brief shows. */
export function conflictNote(transaction: Transaction, label: string) {
  if (label === 'Place') {
    const plaid = [transaction.location?.city, transaction.location?.region]
      .filter(Boolean)
      .join(', ')
    return `The bank’s text says ${placeLabel(transaction.place)}; Plaid says ${plaid}. Brief shows the bank’s text.`
  }
  if (label === 'Amount' && transaction.quality?.statementAmount != null)
    return `Plaid shows ${formatCurrency(transaction.amount)}; the statement shows ${formatCurrency(transaction.quality.statementAmount)}. The statement is the issuer’s final record.`
  return `Sources disagree on the ${label.toLowerCase()}.`
}

export function bankDetails(transaction: FinanceSnapshot['transactions'][number]) {
  const rows: { label: string; value: string; raw?: boolean }[] = []
  if (transaction.originalDescription)
    rows.push({ label: 'Raw text', value: transaction.originalDescription, raw: true })
  for (const counterparty of transaction.counterparties ?? []) {
    rows.push({
      label: counterpartyLabels[counterparty.type ?? ''] ?? 'Counterparty',
      value: [counterparty.name, confidenceLabels[counterparty.confidence ?? '']]
        .filter(Boolean)
        .join(' · '),
    })
  }
  // What the card issuer's imported export says, beside Plaid's view.
  const issuer = transaction.issuer
  if (issuer?.name) rows.push({ label: 'Issuer’s name', value: issuer.name, raw: true })
  if (issuer?.address || issuer?.city)
    rows.push({
      label: 'Issuer’s address',
      value: [
        issuer.address,
        [issuer.city, issuer.region].filter(Boolean).join(' '),
        issuer.postalCode,
      ]
        .filter(Boolean)
        .join(', '),
      raw: true,
    })
  if (issuer?.category) rows.push({ label: 'Issuer’s category', value: issuer.category })
  if (issuer?.phone) rows.push({ label: 'Merchant phone', value: issuer.phone })
  if (transaction.business?.website)
    rows.push({ label: 'Website', value: transaction.business.website })
  if (transaction.accountOwner) rows.push({ label: 'Card holder', value: transaction.accountOwner })
  if (transaction.checkNumber)
    rows.push({ label: 'Check', value: `No. ${transaction.checkNumber}` })
  // What the bank itself reported, which the descriptor's place can overrule.
  const location = transaction.location
  if (location && (location.address || location.city || location.region || location.postalCode)) {
    const coordinates =
      location.lat != null && location.lon != null
        ? `${location.lat.toFixed(4)}, ${location.lon.toFixed(4)}`
        : ''
    rows.push({
      label: 'Location',
      value: [activityLocationText(location), coordinates].filter(Boolean).join(' · '),
    })
  }
  if (transaction.location?.storeNumber)
    rows.push({ label: 'Store', value: `No. ${transaction.location.storeNumber}` })
  if (transaction.categoryConfidence && confidenceLabels[transaction.categoryConfidence])
    rows.push({
      label: 'Category',
      value: `Plaid · ${confidenceLabels[transaction.categoryConfidence]}`,
    })
  if (transaction.transactionCode)
    rows.push({ label: 'Kind', value: transaction.transactionCode.replaceAll('_', ' ') })
  return rows
}
