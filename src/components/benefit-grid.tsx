import { useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'
import { toast } from 'sonner'

import { useFinanceEdits } from '../hooks/use-finance-edits'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { ActivityItem } from '../lib/activity'
import { benefitNote, benefitStatus, creditLagNote } from '../lib/benefit-copy'
import {
  benefitRows,
  benefitYearTotals,
  capturedForOthers,
  plannedAmount,
  benefitYears,
  gridCells,
  periodStateText,
  periodSummary,
  periodsIn,
} from '../lib/benefit-grid'
import { financeSettings } from '../lib/finance-settings'
import {
  countOf,
  formatActivityName,
  formatCurrency,
  formatCurrencyRange,
  formatProseCurrency,
} from '../lib/format'
import { shortcutInput } from '../lib/keyboard'
import { getExternalLogosEnabled } from '../lib/logos'
import type {
  BenefitActivity,
  BenefitHistory,
  BenefitPeriod,
  BenefitReview,
  BenefitWindow,
  FinanceSnapshot,
  Transaction,
} from '../lib/schema'
import { benefitReviewItems, formatActivityDate } from '../lib/spending'
import { ActivityList } from './activity-list'
import { ExternalLink } from './external-link'
import { ArrowUpRight, ChevronRight, X } from './icons'
import { Keys } from './keys'
import { BenefitMark, FeeRecovery } from './platinum-benefits'
import { TransactionDialog } from './transaction-dialog'
import { Button, Card, ChartRangeSelect, EmptyState, Metric } from './ui'
import { PopupContainerContext } from './ui/popup-container'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/** What the inspector shows: one benefit, focused on a period, or the matches to review. */
export type BenefitInspection = { benefit?: string; period?: string; review?: true }

/** A cell's few characters, in whole dollars; its accessible name carries the cents. */
function cellText(period: BenefitPeriod) {
  switch (period.state) {
    case 'full':
    case 'settling':
      return formatProseCurrency(period.credited)
    case 'partial':
      return `${formatProseCurrency(period.credited)} of ${formatProseCurrency(period.cap)}`
    case 'missed':
      return `${formatProseCurrency(period.unused?.high ?? period.cap)} missed`
    case 'open':
      return period.left ? `${formatProseCurrency(period.left.high)} left` : 'Open'
    case 'future':
      return plannedAmount(period) > 0 ? formatProseCurrency(plannedAmount(period)) : ''
    case 'likely-used':
      return 'Used'
    case 'before-history':
      return period.credited ? formatProseCurrency(period.credited) : ''
    default:
      return ''
  }
}

function GridCell({
  period,
  activity,
  onOpen,
}: {
  period: BenefitPeriod
  activity: Map<string, BenefitActivity>
  onOpen: () => void
}) {
  const share = (value: number) =>
    period.cap > 0 ? Math.min(1, Math.max(0, value / period.cap)) : 0
  const credited = period.state === 'likely-used' ? 1 : share(period.credited)
  const planned = Math.min(1 - credited, share(plannedAmount(period)))
  const text = cellText(period)
  const evidence = period.activityIds.flatMap((id) => activity.get(id) ?? [])
  return (
    <Tooltip>
      <TooltipTrigger
        render={
          <button
            type="button"
            className={`benefit-cell is-${period.state}`}
            aria-label={`${period.label}: ${periodStateText[period.state]}. ${periodSummary(period)}`}
            onClick={onOpen}
          />
        }
      >
        <span className="benefit-cell-track" aria-hidden="true">
          <span className="benefit-cell-fill" style={{ width: `${credited * 100}%` }} />
          {planned > 0 ? (
            <span
              className="benefit-cell-planned"
              style={{ left: `${credited * 100}%`, width: `${planned * 100}%` }}
            />
          ) : null}
        </span>
        {text ? <span className="benefit-cell-text sensitive-value">{text}</span> : null}
      </TooltipTrigger>
      <TooltipContent className="treemap-tooltip" sideOffset={8}>
        <div className="treemap-tooltip-heading">
          <strong>
            {period.label} · {periodStateText[period.state]}
          </strong>
          <span className="sensitive-value">
            {formatCurrency(period.credited)} of {formatCurrency(period.cap)}
          </span>
        </div>
        {evidence.length || period.planned?.length ? (
          <div className="treemap-tooltip-composition">
            <ul>
              {evidence.slice(0, 6).map((item) => (
                <li key={item.id}>
                  <span>
                    {formatActivityName(item.merchant)} · {formatActivityDate(item.date)}
                    {item.pending ? ' · pending' : ''}
                  </span>
                  <strong className="sensitive-value">{formatCurrency(item.amount)}</strong>
                </li>
              ))}
              {(period.planned ?? []).map((item) => (
                <li key={`${item.title}:${item.date}`} className="muted">
                  <span>
                    {formatActivityName(item.title)} · {formatActivityDate(item.date)} ·{' '}
                    {item.source === 'purchase' ? 'credit due' : 'planned'}
                  </span>
                  <strong className="sensitive-value">+{formatCurrency(item.amount)}</strong>
                </li>
              ))}
              {evidence.length > 6 ? (
                <li className="muted">+{evidence.length - 6} more · Open to view all</li>
              ) : null}
            </ul>
          </div>
        ) : (
          <p className="benefit-cell-tip-empty">{periodSummary(period)}</p>
        )}
      </TooltipContent>
    </Tooltip>
  )
}

function RowName({
  id,
  name,
  waiting = 0,
  onOpen,
}: {
  id: string
  name: string
  waiting?: number
  onOpen: () => void
}) {
  return (
    <th scope="row">
      <button type="button" className="benefit-row-name" data-keyboard-open onClick={onOpen}>
        <BenefitMark id={id} name={name} externalLogosEnabled={getExternalLogosEnabled()} />
        <span>{name}</span>
        {waiting ? (
          <span className="activity-badge is-warning">
            {waiting}
            <span className="sr-only"> to review</span>
          </span>
        ) : null}
      </button>
    </th>
  )
}

function GridRow({
  benefit,
  year,
  waiting,
  activity,
  onOpen,
}: {
  benefit: BenefitHistory
  year: number
  waiting: number
  activity: Map<string, BenefitActivity>
  onOpen: (period?: string) => void
}) {
  const totals = benefitYearTotals([benefit], year)
  return (
    <tr data-keyboard-row tabIndex={-1} aria-label={benefit.name}>
      <RowName id={benefit.id} name={benefit.name} waiting={waiting} onOpen={() => onOpen()} />
      {gridCells(periodsIn(benefit, year)).map((cell, index) =>
        cell.kind === 'gap' ? (
          <td key={`gap-${index}`} colSpan={cell.span} />
        ) : (
          <td key={cell.period.start} colSpan={cell.span}>
            <GridCell
              period={cell.period}
              activity={activity}
              onOpen={() => onOpen(cell.period.start)}
            />
          </td>
        ),
      )}
      <td className="benefit-grid-figure sensitive-value">
        {benefit.external ? null : formatCurrency(totals.captured)}
      </td>
      <td
        className={`benefit-grid-figure sensitive-value${totals.missed.high > 0 ? ' negative' : ''}`}
      >
        {benefit.external ? null : formatCurrencyRange(totals.missed)}
      </td>
      <td className="benefit-grid-figure sensitive-value">
        {benefit.external || !totals.planned ? null : formatCurrency(totals.planned)}
      </td>
    </tr>
  )
}

/** A renewal or per-purchase benefit has no periods: one cell carries its current state. */
function WindowRow({ benefit, onOpen }: { benefit: BenefitWindow; onOpen: () => void }) {
  return (
    <tr data-keyboard-row tabIndex={-1} aria-label={benefit.name}>
      <RowName id={benefit.id} name={benefit.name} onOpen={onOpen} />
      <td colSpan={12}>
        <button type="button" className="benefit-cell is-renewal" onClick={onOpen}>
          <span className="benefit-cell-text sensitive-value">
            {benefitStatus(benefit).text} · {benefitNote(benefit)}
          </span>
        </button>
      </td>
      <td className="benefit-grid-figure sensitive-value">
        {formatProseCurrency(benefit.creditedAmount)}
      </td>
      <td />
      <td />
    </tr>
  )
}

const reviewQuestion = (item: BenefitReview, name: string) => {
  switch (item.kind) {
    case 'no-purchase':
      return `Which purchase earned this ${name} credit?`
    case 'unverified-credit':
      return `Is this the ${name} credit, or a return?`
    case 'unassigned-credit':
      return 'Unrecognized credit. Which benefit?'
    default:
      return `No ${name} credit came. Was it eligible?`
  }
}

/**
 * A possible match and the decision it needs, in the inspector's flag style. Each decision saves
 * a transaction annotation and can be undone from its confirmation.
 */
function ReviewFlag({
  item,
  data,
  transactions,
  onOpen,
}: {
  item: BenefitReview
  data: FinanceSnapshot
  transactions: Map<string, Transaction>
  onOpen: (id: string) => void
}) {
  const { annotate } = useFinanceEdits()
  const [pending, setPending] = useState(false)
  const [failed, setFailed] = useState(false)
  const options = data.insights?.benefitOptions ?? []
  const [assignTo, setAssignTo] = useState(options[0]?.id ?? '')
  const name =
    options.find(({ id }) => id === item.benefitId)?.name ??
    data.insights?.benefits?.history.find(({ id }) => id === item.benefitId)?.name ??
    'benefit'
  const transaction = transactions.get(item.transactionId)
  const suggestion = item.suggestionId ? transactions.get(item.suggestionId) : undefined
  if (!transaction) return null

  const decide = (target: string, change: Parameters<typeof annotate>[0][string], done: string) => {
    setPending(true)
    setFailed(false)
    annotate({ [target]: change })
      .then(() =>
        toast.success(done, {
          action: {
            label: 'Undo',
            onClick: () =>
              void annotate({ [target]: { clearBenefit: true } }).catch(() =>
                toast.error('Couldn’t undo that'),
              ),
          },
        }),
      )
      .catch(() => {
        setPending(false)
        setFailed(true)
      })
  }
  const evidence = (value: Transaction, lead = '') => (
    <button type="button" className="benefit-flag-transaction" onClick={() => onOpen(value.id)}>
      {`${lead}${formatActivityName(value.merchant)} · ${formatActivityDate(value.postedOn ?? value.date)} · `}
      <span className="sensitive-value">{formatCurrency(value.amount)}</span>
    </button>
  )
  const action = (label: string, run: () => void, primary = false) => (
    <Button
      size="compact"
      variant={primary ? 'secondary' : 'ghost'}
      data-keyboard-open={primary || undefined}
      disabled={pending}
      aria-busy={(pending && primary) || undefined}
      onClick={run}
    >
      {label}
    </Button>
  )

  return (
    <div
      className="transaction-details-flag benefit-flag"
      role="note"
      data-keyboard-row
      tabIndex={-1}
    >
      <div className="benefit-flag-text">
        <p>{reviewQuestion(item, name)}</p>
        {evidence(transaction)}
        {suggestion ? evidence(suggestion, 'Likely ') : null}
      </div>
      <div className="benefit-flag-actions">
        {item.kind === 'no-purchase' && suggestion ? (
          <>
            {action(
              `Link ${formatActivityName(suggestion.merchant)}`,
              () =>
                decide(
                  suggestion.id,
                  { benefitId: item.benefitId },
                  `Linked ${formatActivityName(suggestion.merchant)}`,
                ),
              true,
            )}
            {action('No purchase', () =>
              decide(transaction.id, { benefitConfirmed: true }, 'Kept without a purchase'),
            )}
          </>
        ) : null}
        {item.kind === 'unverified-credit' ? (
          <>
            {action(
              'It’s the credit',
              () => decide(transaction.id, { benefitConfirmed: true }, `Counted toward ${name}`),
              true,
            )}
            {action('A return', () =>
              decide(transaction.id, { benefitConfirmed: false }, 'Kept as a return'),
            )}
          </>
        ) : null}
        {item.kind === 'unassigned-credit' ? (
          <>
            <ChartRangeSelect
              label="Benefit for this credit"
              options={options.map(({ id, name: label }) => ({
                value: id,
                label,
                accessibleLabel: label,
              }))}
              value={assignTo}
              onValueChange={setAssignTo}
            />
            {action(
              'Assign',
              () =>
                decide(
                  transaction.id,
                  { benefitId: assignTo },
                  `Counted toward ${options.find(({ id }) => id === assignTo)?.name ?? 'the benefit'}`,
                ),
              true,
            )}
            {action('Not a benefit', () =>
              decide(transaction.id, { benefitConfirmed: false }, 'Not a benefit credit'),
            )}
          </>
        ) : null}
        {item.kind === 'not-eligible' ? (
          <>
            {action(
              'Not eligible',
              () => decide(transaction.id, { benefitConfirmed: false }, `Removed from ${name}`),
              true,
            )}
            {action('It was', () =>
              decide(transaction.id, { benefitConfirmed: true }, 'Kept · check enrollment'),
            )}
          </>
        ) : null}
      </div>
      {failed ? (
        <p className="transaction-details-error" role="alert">
          Couldn’t save. Try again.
        </p>
      ) : null}
    </div>
  )
}

/**
 * The right-docked inspector, the transaction inspector's dialog. App shortcuts pause while it
 * is open, so it moves through its own rows: J and K, Return to open, Escape to close.
 */
function Inspector({
  open,
  label,
  onClose,
  children,
}: {
  open: boolean
  label: string
  onClose: () => void
  children: ReactNode
}) {
  const [dialog, setDialog] = useState<HTMLDialogElement | null>(null)
  useEffect(() => {
    if (!dialog || typeof dialog.showModal !== 'function') return
    if (open && !dialog.open) dialog.showModal()
    else if (!open && dialog.open) dialog.close()
  }, [dialog, open])
  useEffect(() => {
    if (!open || !dialog) return undefined
    const onKeyDown = (event: KeyboardEvent) => {
      // A transaction opened from here sits on top and handles its own keys.
      if (
        event.defaultPrevented ||
        event.metaKey ||
        event.ctrlKey ||
        event.altKey ||
        shortcutInput(event.target) ||
        !dialog.contains(document.activeElement)
      )
        return
      const rows = [...dialog.querySelectorAll<HTMLElement>('[data-keyboard-row]')].filter(
        (row) => row.getClientRects().length > 0,
      )
      const current = document.activeElement?.closest<HTMLElement>('[data-keyboard-row]')
      const step = ['j', 'ArrowDown'].includes(event.key)
        ? 1
        : ['k', 'ArrowUp'].includes(event.key)
          ? -1
          : 0
      if (step) {
        event.preventDefault()
        const index = current ? rows.indexOf(current) : step > 0 ? -1 : rows.length
        const next = rows[Math.max(0, Math.min(rows.length - 1, index + step))]
        next?.focus()
        next?.scrollIntoView({ block: 'nearest' })
      } else if (event.key === 'Enter' && current && document.activeElement === current) {
        event.preventDefault()
        const target = current.matches('[data-keyboard-open]')
          ? current
          : current.querySelector<HTMLElement>('[data-keyboard-open]')
        target?.click()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [dialog, open])
  return (
    <dialog
      ref={setDialog}
      className="transaction-dialog"
      aria-label={label}
      onCancel={(event) => {
        event.preventDefault()
        onClose()
      }}
      onClick={(event) => {
        if (event.target === event.currentTarget) onClose()
      }}
    >
      {open ? (
        <PopupContainerContext.Provider value={dialog}>
          <article className="transaction-details">
            {children}
            <footer className="command-footer transaction-details-keys" aria-label="Keyboard">
              <span className="command-footer-hint">
                Next <Keys label="J" /> <Keys label="K" />
              </span>
              <span className="command-footer-hint">
                Open <Keys label="Return" />
              </span>
              <span className="command-footer-hint">
                Close <Keys label="Esc" />
              </span>
            </footer>
          </article>
        </PopupContainerContext.Provider>
      ) : null}
    </dialog>
  )
}

function InspectorHeader({
  mark,
  title,
  detail,
  onClose,
}: {
  mark?: ReactNode
  title: string
  detail: ReactNode
  onClose: () => void
}) {
  return (
    <header
      className={`transaction-details-header benefit-inspector-header${mark ? '' : ' benefit-header-plain'}`}
    >
      {mark}
      <div>
        {/* The title takes focus on open, so no control starts out ringed. */}
        <h2 tabIndex={-1} autoFocus>
          {title}
        </h2>
        <p>{detail}</p>
      </div>
      <Button
        icon={X}
        size="icon-compact"
        variant="ghost"
        aria-label="Close details"
        data-command="benefits.close"
        onClick={onClose}
      />
    </header>
  )
}

/** One benefit's year: what it returned, what needs a decision, how to use it, and each period. */
/**
 * Whether you use a benefit at all. A perk you never want shouldn't count as missed or nag, so
 * this hides it from Missed, use-first, and reminders, as Settings does; its credits still count
 * toward the fee.
 */
function BenefitUse({ id, name, data }: { id: string; name: string; data: FinanceSnapshot }) {
  const { updateSettings } = useFinanceEdits()
  const [pending, setPending] = useState(false)
  const [failed, setFailed] = useState(false)
  const using = !financeSettings(data).hiddenBenefitIds.includes(id)
  const toggle = useCallback(() => {
    setPending(true)
    setFailed(false)
    updateSettings((current) => ({
      ...current,
      hiddenBenefitIds: current.hiddenBenefitIds.includes(id)
        ? current.hiddenBenefitIds.filter((hidden) => hidden !== id)
        : [...current.hiddenBenefitIds, id],
    }))
      .catch(() => setFailed(true))
      .finally(() => setPending(false))
  }, [id, updateSettings])
  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'benefits.use.toggle',
          title: using ? `Not using ${name}` : `Track ${name} again`,
          search: 'benefits hide not using track missed reminders',
          group: 'Here',
          scope: 'page',
          disabled: pending,
          disabledReason: pending ? 'Saving' : undefined,
          run: toggle,
        },
      ],
      [name, pending, toggle, using],
    ),
  )
  return (
    <div className="benefit-use">
      <Button
        size="compact"
        variant="ghost"
        data-command="benefits.use.toggle"
        disabled={pending}
        aria-busy={pending || undefined}
        onClick={toggle}
      >
        {using ? 'Not using this' : 'Track this again'}
      </Button>
      {using ? null : <small>Hidden from Missed and reminders</small>}
      {failed ? (
        <p className="transaction-details-error" role="alert">
          Couldn’t save. Try again.
        </p>
      ) : null}
    </div>
  )
}

function BenefitDetail({
  benefit,
  current,
  year,
  focus,
  review,
  data,
  activities,
  transactions,
  onOpenTransaction,
  onClose,
}: {
  benefit?: BenefitHistory
  current?: BenefitWindow
  year: number
  focus?: string
  review: BenefitReview[]
  data: FinanceSnapshot
  activities: Map<string, ActivityItem>
  transactions: Map<string, Transaction>
  onOpenTransaction: (id: string) => void
  onClose: () => void
}) {
  const periods = benefit
    ? periodsIn(benefit, year)
        .filter(({ state }) => state !== 'future')
        .toReversed()
    : []
  const [open, setOpen] = useState(
    () =>
      new Set(
        [focus ?? periods.find(({ state }) => state !== 'before-history')?.start ?? ''].filter(
          Boolean,
        ),
      ),
  )
  const name = benefit?.name ?? current?.name ?? 'Benefit'
  const id = benefit?.id ?? current?.id ?? ''
  const totals = benefitYearTotals(benefit ? [benefit] : [], year)
  const rows = (ids: string[]) =>
    ids.flatMap((activityId) => activities.get(`spending:${activityId}`) ?? [])
  const likelyUsed = periods.filter(({ state }) => state === 'likely-used').length
  return (
    <>
      <InspectorHeader
        mark={<BenefitMark id={id} name={name} externalLogosEnabled={getExternalLogosEnabled()} />}
        title={name}
        detail={`${current?.limit ?? year}${benefit?.recurring ? ' · via subscription' : ''}`}
        onClose={onClose}
      />
      {benefit?.external ? (
        <p className="benefit-detail-note">
          {`Likely used in ${likelyUsed} of ${periods.length} months`}
        </p>
      ) : benefit ? (
        <div className="benefit-metrics benefit-detail-metrics">
          <Metric
            label="Captured"
            value={<span className="sensitive-value">{formatCurrency(totals.captured)}</span>}
          />
          <Metric
            label="Missed"
            tone={totals.missed.high > 0 ? 'negative' : undefined}
            value={<span className="sensitive-value">{formatCurrencyRange(totals.missed)}</span>}
          />
          {totals.left.high > 0 ? (
            <Metric
              label="Left now"
              value={<span className="sensitive-value">{formatCurrencyRange(totals.left)}</span>}
            />
          ) : null}
          {totals.planned > 0 ? (
            <Metric
              label="Estimated"
              value={<span className="sensitive-value">{formatCurrency(totals.planned)}</span>}
            />
          ) : null}
        </div>
      ) : current ? (
        <p className="benefit-detail-note">{benefitNote(current)}</p>
      ) : null}
      <div className="transaction-details-body">
        {review.length ? (
          <div className="benefit-flags">
            {review.map((item) => (
              <ReviewFlag
                key={`${item.kind}:${item.transactionId}`}
                item={item}
                data={data}
                transactions={transactions}
                onOpen={onOpenTransaction}
              />
            ))}
          </div>
        ) : null}
        {id ? <BenefitUse id={id} name={name} data={data} /> : null}
        {current ? (
          <dl className="transaction-details-facts">
            <div>
              <dt>How</dt>
              <dd>{current.instruction}</dd>
            </div>
            {current.deadlineNote ? (
              <div>
                <dt>Cutoff</dt>
                <dd>{current.deadlineNote}</dd>
              </div>
            ) : null}
            <div>
              <dt>Posting</dt>
              <dd>{creditLagNote(current)}</dd>
            </div>
            <div>
              <dt>Terms</dt>
              <dd>
                <ExternalLink
                  className="transaction-details-link"
                  href={current.url}
                  data-command="benefits.terms"
                >
                  Official benefit terms
                  <ArrowUpRight size={12} aria-hidden="true" />
                </ExternalLink>
              </dd>
            </div>
          </dl>
        ) : null}
        <section
          className="transaction-details-history benefit-periods"
          aria-label={`${name} periods`}
        >
          <h3>
            {benefit ? year : 'Activity'}
            <span>
              {benefit
                ? `${periods.length} ${periods.length === 1 ? 'period' : 'periods'}`
                : `${current?.activity.length ?? 0} matched`}
            </span>
          </h3>
          {benefit ? (
            <ul>
              {periods.map((period) => {
                const expanded = open.has(period.start)
                const items = rows(period.activityIds)
                return (
                  <li key={period.start} data-keyboard-row tabIndex={-1}>
                    <button
                      type="button"
                      className="benefit-period-row"
                      aria-expanded={expanded}
                      data-keyboard-open
                      onClick={() =>
                        setOpen((shown) => {
                          const next = new Set(shown)
                          if (next.has(period.start)) next.delete(period.start)
                          else next.add(period.start)
                          return next
                        })
                      }
                    >
                      <ChevronRight size={12} aria-hidden="true" className="benefit-chevron" />
                      <span>{period.label}</span>
                      <small className={`is-${period.state}`}>{periodSummary(period)}</small>
                      <strong className="sensitive-value">
                        {benefit.external || period.state === 'before-history'
                          ? null
                          : formatCurrency(period.credited)}
                      </strong>
                    </button>
                    {expanded ? (
                      <div className="benefit-period-activity">
                        {items.length ? (
                          <ActivityList
                            activities={items}
                            referenceIso={data.updatedAt}
                            onOpen={(activity) =>
                              onOpenTransaction(activity.id.replace(/^spending:/, ''))
                            }
                          />
                        ) : period.planned?.length ? null : (
                          <p className="benefit-period-empty">
                            {benefit.external ? 'No Uber charge' : 'No credit or purchase'}
                          </p>
                        )}
                        {period.planned?.length ? (
                          <ul
                            className="benefit-planned"
                            aria-label={`Planned for ${period.label}`}
                          >
                            {period.planned.map((item) => (
                              <li key={`${item.title}:${item.date}`}>
                                <span>
                                  {formatActivityName(item.title)}
                                  <small>
                                    {formatActivityDate(item.date)} ·{' '}
                                    {item.source === 'purchase'
                                      ? 'credit due for this purchase'
                                      : 'expected charge'}
                                  </small>
                                </span>
                                <strong className="sensitive-value">
                                  +{formatCurrency(item.amount)}
                                </strong>
                              </li>
                            ))}
                          </ul>
                        ) : null}
                      </div>
                    ) : null}
                  </li>
                )
              })}
            </ul>
          ) : current?.activity.length ? (
            <ActivityList
              activities={rows(current.activity.map(({ id: activityId }) => activityId))}
              referenceIso={data.updatedAt}
              onOpen={(activity) => onOpenTransaction(activity.id.replace(/^spending:/, ''))}
            />
          ) : (
            <EmptyState>No credit or purchase</EmptyState>
          )}
        </section>
      </div>
    </>
  )
}

/**
 * Benefits for the saved spending card as a year grid: one row per benefit, one cell per period,
 * each opening the benefit's evidence in the inspector. Rust settles every period and finds the
 * matches to review; this only lays them out and sums cents.
 */
export function BenefitGrid({
  data,
  year,
  today,
  inspection,
  activities,
  onYear,
  onInspect,
}: {
  data: FinanceSnapshot
  year: number
  today: string
  inspection: BenefitInspection
  activities: Map<string, ActivityItem>
  onYear: (year: number) => void
  onInspect: (next: BenefitInspection) => void
}) {
  const card = data.insights?.benefits
  const hidden = financeSettings(data).hiddenBenefitIds
  const currentYear = Number(today.slice(0, 4))
  const years = benefitYears(card, today)
  const rows = benefitRows(card, year, hidden)
  const special =
    year === currentYear
      ? (card?.windows ?? []).filter(
          ({ id, cadence }) =>
            (cadence === 'renewal' || cadence === 'purchase') && !hidden.includes(id),
        )
      : []
  const totals = benefitYearTotals(rows, year)
  const review = benefitReviewItems(card, hidden)
  const [openTransaction, setOpenTransaction] = useState<string>()
  const transactions = useMemo(
    () => new Map(data.transactions.map((transaction) => [transaction.id, transaction])),
    [data.transactions],
  )
  const waiting = (id: string) => review.filter(({ benefitId }) => benefitId === id).length
  const activity = useMemo(
    () => new Map((card?.activity ?? []).map((item) => [item.id, item])),
    [card?.activity],
  )
  const others = useMemo(
    () => new Set(data.transactions.filter(({ other }) => other).map(({ id }) => id)),
    [data.transactions],
  )

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'benefits.review',
          title: 'Review benefit matches',
          search: 'benefits review matches confirm credits verify',
          group: 'Here',
          scope: 'page',
          run: () => onInspect({ review: true }),
        },
        {
          id: 'benefits.close',
          title: 'Close benefit details',
          search: 'benefits close inspector details',
          group: 'Here',
          scope: 'page',
          run: () => onInspect({}),
        },
        {
          id: 'benefits.terms',
          title: 'Open benefit terms',
          search: 'benefits terms official',
          group: 'Here',
          scope: 'page',
          run: () =>
            document.querySelector<HTMLElement>('[data-command="benefits.terms"]')?.click(),
        },
        ...years.map<AppAction>((option) => ({
          id: `benefits.year.${option}`,
          title: option === currentYear ? 'Show this year’s benefits' : `Show ${option} benefits`,
          search: `benefits year ${option}`,
          group: 'Here',
          scope: 'page',
          run: () => onYear(option),
        })),
      ],
      [currentYear, onInspect, onYear, years],
    ),
  )

  const inspected = inspection.benefit
    ? {
        benefit: card?.history.find(({ id }) => id === inspection.benefit),
        current: card?.windows.find(({ id }) => id === inspection.benefit),
      }
    : undefined
  const close = () => onInspect({})
  const group = (title: string, list: BenefitHistory[]) =>
    list.length ? (
      <>
        <tr className="benefit-grid-group">
          <th scope="rowgroup" colSpan={16}>
            {title}
          </th>
        </tr>
        {list.map((benefit) => (
          <GridRow
            key={benefit.id}
            benefit={benefit}
            year={year}
            waiting={waiting(benefit.id)}
            activity={activity}
            onOpen={(period) => onInspect({ benefit: benefit.id, period })}
          />
        ))}
      </>
    ) : null

  // Your share and theirs add up to the year's captured credits.
  const forOthers = capturedForOthers(rows, year, activity, others)

  return (
    <>
      <Card className="benefit-grid-card">
        {/* Whether the card pays for itself leads; its year runs from the fee, not the calendar. */}
        {card?.fee && year === currentYear ? <FeeRecovery fee={card.fee} /> : null}
        <div className="benefit-grid-top">
          <div className="benefit-metrics">
            <Metric
              label={`For you in ${year}`}
              value={
                <span className="sensitive-value">
                  {formatCurrency(totals.captured - forOthers)}
                </span>
              }
            />
            {forOthers > 0 ? (
              <Metric
                label="For others"
                value={<span className="sensitive-value">{formatCurrency(forOthers)}</span>}
              />
            ) : null}
            <Metric
              label="Missed"
              tone={totals.missed.high > 0 ? 'negative' : undefined}
              value={<span className="sensitive-value">{formatCurrencyRange(totals.missed)}</span>}
            />
            {totals.planned > 0 ? (
              <Metric
                label="Estimated"
                value={<span className="sensitive-value">{formatCurrency(totals.planned)}</span>}
              />
            ) : null}
            {review.length ? (
              <Metric
                label="To review"
                value={
                  <button
                    type="button"
                    className="benefit-review-link"
                    data-command="benefits.review"
                    onClick={() => onInspect({ review: true })}
                  >
                    {countOf(review.length, 'match', 'matches')}
                  </button>
                }
              />
            ) : null}
          </div>
          <ChartRangeSelect
            label="Benefits year"
            command="benefits.year."
            options={years.map((option) => ({
              value: option,
              label: option === currentYear ? 'This year' : String(option),
              accessibleLabel: option === currentYear ? 'This year' : String(option),
            }))}
            value={year}
            onValueChange={onYear}
          />
        </div>
        {rows.length || special.length ? (
          <div className="benefit-grid-scroll">
            <table className="benefit-grid" aria-label={`Benefits in ${year}`} data-keyboard-region>
              <thead>
                <tr>
                  <th scope="col">
                    <span className="sr-only">Benefit</span>
                  </th>
                  {months.map((month, index) => (
                    <th
                      key={month}
                      scope="col"
                      className={
                        year === currentYear && index === Number(today.slice(5, 7)) - 1
                          ? 'is-now'
                          : undefined
                      }
                    >
                      {month}
                    </th>
                  ))}
                  <th scope="col" className="benefit-grid-figure">
                    Captured
                  </th>
                  <th scope="col" className="benefit-grid-figure">
                    Missed
                  </th>
                  <th scope="col" className="benefit-grid-figure">
                    Estimated
                  </th>
                </tr>
              </thead>
              <tbody>
                {group(
                  'Use by deadline',
                  rows.filter(({ recurring }) => !recurring),
                )}
                {group(
                  'Recurring',
                  rows.filter(({ recurring }) => recurring),
                )}
                {special.length ? (
                  <>
                    <tr className="benefit-grid-group">
                      <th scope="rowgroup" colSpan={16}>
                        Renewals & purchases
                      </th>
                    </tr>
                    {special.map((benefit) => (
                      <WindowRow
                        key={benefit.id}
                        benefit={benefit}
                        onOpen={() => onInspect({ benefit: benefit.id })}
                      />
                    ))}
                  </>
                ) : null}
              </tbody>
            </table>
          </div>
        ) : (
          <EmptyState>{`No benefits in ${year}`}</EmptyState>
        )}
      </Card>
      <Inspector
        open={Boolean(inspection.review || inspected?.benefit || inspected?.current)}
        label={
          inspection.review
            ? 'To review'
            : (inspected?.benefit?.name ?? inspected?.current?.name ?? 'Benefit')
        }
        onClose={close}
      >
        {inspection.review ? (
          <>
            <InspectorHeader
              title="To review"
              detail={review.length ? countOf(review.length, 'match', 'matches') : 'All settled'}
              onClose={close}
            />
            <div className="transaction-details-body">
              {review.length ? (
                <div className="benefit-flags">
                  {review.map((item) => (
                    <ReviewFlag
                      key={`${item.kind}:${item.transactionId}`}
                      item={item}
                      data={data}
                      transactions={transactions}
                      onOpen={setOpenTransaction}
                    />
                  ))}
                </div>
              ) : (
                <EmptyState>Nothing to review</EmptyState>
              )}
            </div>
          </>
        ) : inspected ? (
          <BenefitDetail
            key={`${inspection.benefit}:${inspection.period ?? ''}:${year}`}
            benefit={inspected.benefit}
            current={inspected.current}
            year={year}
            focus={inspection.period}
            review={review.filter(({ benefitId }) => benefitId === inspection.benefit)}
            data={data}
            activities={activities}
            transactions={transactions}
            onOpenTransaction={setOpenTransaction}
            onClose={close}
          />
        ) : null}
      </Inspector>
      <TransactionDialog
        activity={openTransaction ? activities.get(`spending:${openTransaction}`) : undefined}
        data={data}
        onClose={() => setOpenTransaction(undefined)}
      />
    </>
  )
}
