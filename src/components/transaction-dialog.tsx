import { Link } from '@tanstack/react-router'
import { useId, useMemo, useState } from 'react'

import type { AppAction } from '../lib/action-system'
import { activityLocationText, activityTime, type ActivityItem } from '../lib/activity'
import { documentTitle, transactionDocuments } from '../lib/documents'
import { formatActivityName, formatCurrency, formatProseCurrency, valueTone } from '../lib/format'
import { activityObjectId } from '../lib/objects'
import { placeLabel } from '../lib/places'
import type { FinanceSnapshot, QualityEvidence, Transaction } from '../lib/schema'
import { formatActivityDate } from '../lib/spending'
import { transactionDetail } from '../lib/transaction-detail'
import { ActivityMark } from './activity-list'
import { PlaceMark } from './activity-method-mark'
import { CategoryMark } from './category-mark'
import { ArrowUpRight, FileText, Repeat2, X } from './icons'
import { useShowObject } from './object-details'
import {
  BenefitLinkFlag,
  CategorySuggestionFlag,
  InspectorKeysFooter,
  MerchantHistory,
  RawRecord,
  ReviewFlag,
  TransactionHero,
  VerifiedFact,
  WhereFacts,
} from './transaction-details-sections'
import { TransactionEdits } from './transaction-edits'
import { Button } from './ui'
import { Dialog, dialogAutofocus } from './ui/dialog'
import { usePresence } from './ui/presence'

/**
 * The inspector's own keys: J and K step through the list; C, T, P, N, and S start an edit; F
 * switches between yours and someone else's; R marks a flagged transaction reviewed; U unlinks
 * a benefit match; O opens a transfer's other side; A keeps a suggested category and D declines
 * it. Each edit key presses the control marked
 * with its `data-inspector-key`, when the transaction has one.
 */
const inspectorKeys = [
  { id: 'next', title: 'Next transaction', keys: ['j', 'ArrowDown'] },
  { id: 'previous', title: 'Previous transaction', keys: ['k', 'ArrowUp'] },
  { id: 'category', title: 'Change category', keys: ['c'] },
  { id: 'tag', title: 'Add a tag', keys: ['t'] },
  { id: 'people', title: 'Choose people', keys: ['p'] },
  { id: 'note', title: 'Write a note', keys: ['n'] },
  { id: 'split', title: 'Split', keys: ['s'] },
  { id: 'other', title: 'Switch between mine and someone else’s', keys: ['f'] },
  { id: 'reviewed', title: 'Mark reviewed', keys: ['r'] },
  { id: 'unlink', title: 'Unlink the benefit match', keys: ['u'] },
  { id: 'transfer', title: 'Open the other side of the transfer', keys: ['o'] },
  { id: 'accept', title: 'Keep the suggested category', keys: ['a'] },
  { id: 'reject', title: 'Decline the suggested category', keys: ['d'] },
] as const

const keyLabel = (key: string) =>
  key === 'ArrowDown' ? '↓' : key === 'ArrowUp' ? '↑' : key.toUpperCase()

/** Which inspector key `key` presses, if any. */
export function inspectorKey(key: string) {
  return inspectorKeys.find(({ keys }) => keys.some((bound) => bound === key))?.id
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

  // The inspector's keys run through the app's dispatcher, scoped to this dialog, so they show
  // in the key guide and the palette. They read the dialog when they run, so a key still works
  // after stepping replaces the focused content.
  const showObject = useShowObject()
  const objectId = activity ? activityObjectId(activity.id) : undefined
  const actions = useMemo<AppAction[]>(() => {
    const step = (direction: 1 | -1) => {
      onStep?.(direction)
      requestAnimationFrame(() =>
        dialog?.querySelector<HTMLElement>('.transaction-details-header h2')?.focus(),
      )
    }
    return [
      ...inspectorKeys
        .filter(({ id }) => onStep || (id !== 'next' && id !== 'previous'))
        .map(({ id, title, keys }) => ({
          id: `inspector.${id}`,
          title,
          group: 'Here',
          scope: 'dialog' as const,
          bindings: keys.map((key) => ({ sequence: [key], label: keyLabel(key) })),
          run: () =>
            id === 'next' || id === 'previous'
              ? step(id === 'next' ? 1 : -1)
              : dialog?.querySelector<HTMLElement>(`[data-inspector-key="${id}"]`)?.click(),
        })),
      // ⌘I: everything stored about this transaction, over the inspector.
      ...(objectId
        ? [
            {
              id: 'inspector.details',
              title: 'Show details',
              group: 'Here',
              scope: 'dialog' as const,
              bindings: [{ sequence: ['Meta+i'], label: '⌘I' }],
              run: () => showObject(objectId),
            },
          ]
        : []),
    ]
  }, [dialog, objectId, onStep, showObject])

  return (
    <Dialog
      ref={setDialog}
      open={Boolean(activity)}
      onClose={onClose}
      escapeOnKeyDown
      popups
      actions={actions}
      className="transaction-dialog"
      aria-labelledby={titleId}
    >
      {shown ? (
        <TransactionDetails
          key={shown.id}
          activity={shown}
          data={data}
          titleId={titleId}
          onClose={onClose}
          onReviewed={onReviewed}
          onOpenActivity={onOpenActivity}
        />
      ) : null}
    </Dialog>
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
  const transfer = activity.transfer
  const posted = transaction?.postedOn ?? activity.date
  const time = activityTime(activity)
  const wallet = transaction?.wallet
  const merchant = formatActivityName(activity.title)
  // Documents that state this transaction: a statement lists it, or a pay statement paid it.
  const evidence = transaction ? transactionDocuments(data, transaction.id) : []
  const onStatement = evidence.some(({ kind }) => kind === 'statement')
  const onPayStatement = evidence.some(({ kind }) => kind === 'pay')
  // Who else recorded it: Wallet at the terminal, a statement, or a pay statement.
  const sources = [
    wallet && !activity.pending && !transaction?.id.startsWith('wallet:') ? 'Apple Wallet' : '',
    onStatement ? 'the statement' : onPayStatement ? 'the pay statement' : '',
  ].filter(Boolean)
  const quality = transaction ? evidenceDetails(transaction) : []
  const details = transaction ? bankDetails(transaction) : []
  const walletFacts = transaction ? walletDetails(transaction) : []
  // Details whose sources disagree, until the user marks the transaction reviewed.
  const conflicts =
    transaction?.quality && !transaction.reviewed
      ? quality.filter(({ conflict }) => conflict).map(({ label }) => label)
      : []

  const categoryOrigin =
    detail?.categoryOrigin === 'manual'
      ? 'Set by you'
      : detail?.categoryOrigin === 'suggested'
        ? 'Suggested from your choices'
        : wallet?.categorized
          ? 'From the card network'
          : transaction?.providerCategory && transaction.providerCategory !== activity.category
            ? `Bank says ${transaction.providerCategory}`
            : 'From the bank'

  return (
    <article className="transaction-details">
      <header className="transaction-details-header">
        <ActivityMark activity={activity} />
        <div>
          {/* The title takes focus on open, so no control starts out ringed. */}
          <h2 id={titleId} tabIndex={-1} {...dialogAutofocus}>
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

      <TransactionHero
        activity={activity}
        detail={detail}
        posted={posted}
        occurred={transaction?.occurredOn}
        atTime={time ? ` at ${time}` : ''}
        disagree={conflicts.length > 0}
        onClose={onClose}
      />

      <div className="transaction-details-body">
        <ReviewFlag
          activity={activity}
          data={data}
          detail={detail}
          transaction={transaction}
          conflictText={
            transaction && conflicts.length
              ? conflicts.map((label) => conflictNote(transaction, label)).join(' ')
              : undefined
          }
          onReviewed={onReviewed}
        />
        <CategorySuggestionFlag data={data} transaction={transaction} />
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
              <WhereFacts activity={activity} transaction={transaction} />
            </>
          )}
          {detail?.expected ? (
            <div>
              <dt>Repeats</dt>
              <dd>
                <span className="transaction-details-value">
                  <Repeat2 size={14} aria-hidden="true" />
                  <span>
                    {detail.expected.frequency}
                    <small>Next expected {formatActivityDate(detail.expected.date)}</small>
                  </span>
                </span>
              </dd>
            </div>
          ) : null}
          {transaction?.dividendType ? (
            <div>
              <dt>Dividend</dt>
              <dd>{transaction.dividendType === 'qualified' ? 'Qualified' : 'Ordinary'}</dd>
            </div>
          ) : null}
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
          {transaction ? (
            <VerifiedFact activity={activity} sources={sources} quality={quality} />
          ) : null}
        </dl>

        {transaction ? (
          <TransactionEdits
            key={transaction.id}
            transaction={transaction}
            data={data}
            categoryNote={categoryOrigin}
          />
        ) : null}

        {detail ? (
          <MerchantHistory
            activity={activity}
            detail={detail}
            merchant={merchant}
            posted={posted}
            titleId={titleId}
          />
        ) : null}

        {walletFacts.length || details.length ? (
          <details className="transaction-details-more">
            <summary>Raw records</summary>
            {walletFacts.length ? <RawRecord title="Apple Wallet" rows={walletFacts} /> : null}
            {details.length ? <RawRecord title="Bank" rows={details} /> : null}
          </details>
        ) : null}
      </div>
      <InspectorKeysFooter detail={detail} />
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

const evidenceLabels: Record<QualityEvidence['basis'], string> = {
  statement: 'Matches the statement',
  statementDiffers: 'The statement shows another amount',
  statementOnly: 'From an imported statement',
  pending: 'Pending; may change when it posts',
  wallet: 'Apple Wallet recorded it',
  merchantCode: 'The card network’s merchant code',
  suggested: 'Brief followed your earlier choices',
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

/** What Apple Wallet recorded that the page doesn't show: its name and the network's identifiers. */
export function walletDetails(transaction: FinanceSnapshot['transactions'][number]) {
  const wallet = transaction.wallet
  const rows: { label: string; value: string; raw?: boolean }[] = []
  if (!wallet) return rows
  const { network } = wallet
  if (wallet.merchant && wallet.merchant !== transaction.merchant)
    rows.push({ label: 'Wallet’s name', value: wallet.merchant })
  if (network?.merchantCode != null)
    rows.push({ label: 'Merchant code', value: String(network.merchantCode), raw: true })
  if (network?.descriptor)
    rows.push({ label: 'Network text', value: network.descriptor, raw: true })
  if (network?.merchantId) rows.push({ label: 'Merchant ID', value: network.merchantId, raw: true })
  if (network?.terminalId) rows.push({ label: 'Terminal ID', value: network.terminalId, raw: true })
  if (network?.entryMode) rows.push({ label: 'Entry mode', value: network.entryMode, raw: true })
  rows.push({ label: 'Wallet ID', value: wallet.recordId, raw: true })
  return rows
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
