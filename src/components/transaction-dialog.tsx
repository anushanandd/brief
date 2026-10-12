import { useEffect, useId, useState } from 'react'

import { useFinanceEdits } from '../hooks/use-finance-edits'
import {
  activityLocationText,
  activityMethod,
  activityPlace,
  type ActivityItem,
} from '../lib/activity'
import { safeExternalUrl } from '../lib/api'
import { formatActivityName, formatCurrency, formatProseCurrency, valueTone } from '../lib/format'
import { shortcutInput } from '../lib/keyboard'
import type { FinanceSnapshot } from '../lib/schema'
import { formatActivityDate } from '../lib/spending'
import { mapsUrl, transactionDetail } from '../lib/transaction-detail'
import { ActivityMark } from './activity-list'
import { PlaceMark } from './activity-method-mark'
import { CategoryMark } from './category-mark'
import { ExternalLink } from './external-link'
import { AlertCircle, ArrowUpRight, Check, MapPinned, Plane, Repeat2, X } from './icons'
import { Keys } from './keys'
import { escapeClosesSettings } from './settings-dialog'
import { TransactionEdits } from './transaction-edits'
import { AnimatedCurrency, Button } from './ui'
import { PopupContainerContext } from './ui/popup-container'

/**
 * The inspector's own keys: J and K step through the list; C, T, and S start an edit; F switches
 * between yours and someone else's; R marks a flagged transaction reviewed.
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
    case 's':
      return 'split'
    case 'f':
      return 'other'
    case 'r':
      return 'reviewed'
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
}: {
  activity: ActivityItem | undefined
  data: FinanceSnapshot
  onClose: () => void
  /** Opens the next (1) or previous (-1) row of the list this was opened from. */
  onStep?: (direction: 1 | -1) => void
  /** A flagged transaction was marked reviewed; a review queue moves on. */
  onReviewed?: (id: string) => void
}) {
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  const titleId = useId()
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
      {activity ? (
        <PopupContainerContext.Provider value={dialog}>
          <TransactionDetails
            key={activity.id}
            activity={activity}
            data={data}
            titleId={titleId}
            onClose={onClose}
            onReviewed={onReviewed}
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
}: {
  activity: ActivityItem
  data: FinanceSnapshot
  titleId: string
  onClose: () => void
  onReviewed?: (id: string) => void
}) {
  const detail = transactionDetail(data, activity.id)
  const transaction = detail?.transaction
  const reference = data.updatedAt
  const place = activityPlace(activity)
  const address = activityLocationText(activity.location)
  const website = activity.website ? safeExternalUrl(activity.website) : undefined
  const method = activityMethod(activity)
  const posted = transaction?.postedOn ?? activity.date
  const occurred = transaction?.occurredOn
  const merchant = formatActivityName(activity.title)
  const coordinates = transaction?.location
  const mapsQuery = address || (coordinates?.lat != null ? merchant : '')
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

  const categoryOrigin =
    detail?.categoryOrigin === 'manual'
      ? 'Set by you'
      : detail?.categoryOrigin === 'rule'
        ? `Rule “${detail.rule?.pattern ?? 'saved rule'}”`
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
            {website ? (
              <>
                <span aria-hidden="true"> · </span>
                <ExternalLink className="transaction-details-link" href={website}>
                  {new URL(website).hostname.replace(/^www\./, '')}
                  <ArrowUpRight size={12} aria-hidden="true" />
                </ExternalLink>
              </>
            ) : null}
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
              Purchased {formatActivityDate(occurred, reference)}
              <span aria-hidden="true"> · </span>
              posted {formatActivityDate(posted, reference)}
            </>
          ) : (
            formatActivityDate(posted, reference)
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
          {transaction?.dividendType ? (
            <span className="transaction-details-tag">
              {transaction.dividendType === 'qualified'
                ? 'Qualified dividend'
                : 'Ordinary dividend'}
            </span>
          ) : null}
          {detail?.benefits.map((benefit) => (
            <span
              className="transaction-details-tag is-positive"
              key={`${benefit.benefitId}:${benefit.kind}`}
            >
              {benefit.kind === 'credit'
                ? `${benefit.benefitName} credit`
                : `Counts toward ${benefit.benefitName}`}
            </span>
          ))}
        </p>
      </section>

      <div className="transaction-details-body">
        {detail?.duplicate || detail?.unusual ? (
          <div className="transaction-details-flag" role="note">
            <p>
              {detail.duplicate ? (
                'Same amount, same day. If you paid once, dispute one with the issuer.'
              ) : (
                <>
                  Usually{' '}
                  <span className="sensitive-value">
                    {formatProseCurrency(Math.abs(detail.unusual!.usual))}
                  </span>{' '}
                  here.
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
              {detail.duplicate ? 'Not a duplicate' : 'Looks right'}
            </Button>
            {reviewFailed ? (
              <p className="transaction-details-error" role="alert">
                Couldn’t save. Try again.
              </p>
            ) : null}
          </div>
        ) : null}
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
          <div>
            <dt>Where</dt>
            <dd>
              <span className="transaction-details-value">
                {place.kind === 'none' ? null : <PlaceMark kind={place.kind} />}
                <span>
                  {place.kind === 'none' ? 'Not reported by the bank' : place.primary}
                  {address && address !== place.primary ? <small>{address}</small> : null}
                  {place.kind !== 'place' && method && method !== place.primary ? (
                    <small>{method}</small>
                  ) : null}
                </span>
              </span>
              {mapsQuery ? (
                <ExternalLink
                  className="transaction-details-link"
                  href={mapsUrl(mapsQuery, coordinates)}
                >
                  <MapPinned size={12} aria-hidden="true" />
                  Open in Maps
                </ExternalLink>
              ) : null}
            </dd>
          </div>
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
                  {formatActivityDate(detail.trip.start, reference)} –{' '}
                  {formatActivityDate(detail.trip.end, reference)} ·{' '}
                  <span className="sensitive-value">
                    {formatProseCurrency(Math.abs(detail.trip.total))}
                  </span>
                </small>
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
                  <span>{formatActivityDate(detail.expected.date, reference)}</span>
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
                <span>{formatActivityDate(posted, reference)}</span>
                <small>This one</small>
                <strong className={`sensitive-value ${valueTone(activity.amount)}`}>
                  {formatCurrency(activity.amount)}
                </strong>
              </li>
              {detail.history.map((item) => (
                <li key={item.id}>
                  <span>{formatActivityDate(item.postedOn ?? item.date, reference)}</span>
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
        <span className="command-footer-hint">
          Category <Keys label="C" />
        </span>
        <span className="command-footer-hint">
          Tag <Keys label="T" />
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
  if (transaction.accountOwner) rows.push({ label: 'Card holder', value: transaction.accountOwner })
  if (transaction.checkNumber)
    rows.push({ label: 'Check', value: `No. ${transaction.checkNumber}` })
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
