import { Popover } from '@base-ui/react/popover'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useMemo, useState } from 'react'
import { toast } from 'sonner'

import { ActivityList } from '../components/activity-list'
import { categoryColor } from '../components/category-mark'
import { PageError, PageLoading } from '../components/data-state'
import { CalendarDays, Check, ChevronDown, ChevronRight } from '../components/icons'
import { Button, Card, EmptyState, Metric, SectionHeading } from '../components/ui'
import { usePopupPortal } from '../components/ui/popup-container'
import { WorkspaceHeader } from '../components/workspace-header'
import { useFinance } from '../hooks/use-finance'
import { accountDisplayName } from '../lib/account-names'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import { buildActivities, type ActivityItem } from '../lib/activity'
import { getEarningsCalendar, isTauri } from '../lib/api'
import {
  agendaDetail,
  agendaLabel,
  calendarEvents,
  calendarList,
  calendarStatus,
  shownCalendars,
  entriesOn,
  eventTime,
  firstOfMonth,
  monthGrid,
  monthOf,
  requestCalendarAccess,
  shiftMonth,
  type CalendarAccess,
  type DayEntry,
} from '../lib/calendar'
import { addDays } from '../lib/dates'
import { errorMessage } from '../lib/errors'
import { financeSettings } from '../lib/finance-settings'
import { formatActivityName, formatCurrency, formatSignedCurrency, valueTone } from '../lib/format'
import { getExternalLogosEnabled } from '../lib/logos'
import type { AgendaItem, FinanceSnapshot } from '../lib/schema'

/** Each kind's color, by what it concerns: cards, bills, money in, investments, travel. */
const kindColor: Record<AgendaItem['kind'] | 'earnings', string> = {
  paymentDue: 'var(--data-1)',
  statementCloses: 'var(--data-1)',
  creditExpiring: 'var(--data-1)',
  bill: 'var(--data-3)',
  subscription: 'var(--data-3)',
  income: 'var(--data-2)',
  dividend: 'var(--data-5)',
  longTerm: 'var(--data-5)',
  washSaleEnds: 'var(--data-5)',
  earnings: 'var(--data-5)',
  trip: 'var(--data-4)',
}
const MAX_LINES = 4
/** What the colors mean, under the month. Purchases wear their category's color. */
const legend = [
  { label: 'Cards', color: kindColor.paymentDue },
  { label: 'Bills', color: kindColor.bill },
  { label: 'Money in', color: kindColor.income },
  { label: 'Investments', color: kindColor.dividend },
  { label: 'Trips', color: kindColor.trip },
]
const monthFormatter = new Intl.DateTimeFormat('en-US', {
  month: 'long',
  year: 'numeric',
  timeZone: 'UTC',
})
const dayFormatter = new Intl.DateTimeFormat('en-US', {
  weekday: 'long',
  month: 'long',
  day: 'numeric',
  timeZone: 'UTC',
})
const weekdays = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat']

/**
 * Calendar: a month of what Brief knows is dated (payments due, statements closing, bills and
 * income, card credits ending, investment dates, trips) and each day's activity, with the
 * user's own calendars beside them once chosen. Rust dates every item; this page lays them out.
 */
export function CalendarPage() {
  const query = useFinance()
  if (query.isLoading) return <PageLoading />
  if (query.isError || !query.data) return <PageError />
  return <CalendarView data={query.data} alphaVantage={query.integrationStatus.alphaVantage} />
}

function CalendarView({ data, alphaVantage }: { data: FinanceSnapshot; alphaVantage: boolean }) {
  const search = useSearch({ from: '/calendar' })
  const navigate = useNavigate({ from: '/calendar' })
  const today = data.insights?.today ?? data.updatedAt.slice(0, 10)
  const day = search.day ?? today
  const month = search.month ?? monthOf(day)
  const days = useMemo(() => monthGrid(month), [month])
  const native = typeof window !== 'undefined' && isTauri()
  const agenda = data.insights?.agenda ?? []
  const names = financeSettings(data).accountNames
  const accountName = (id: string) =>
    accountDisplayName(
      id,
      data.accounts.find((account) => account.id === id)?.name ?? 'Card',
      names,
    )

  const watch = (data.insights?.investmentAttention?.earningsWatch ?? []).toSorted()
  const earnings = useQuery({
    queryKey: ['earnings-calendar', watch],
    queryFn: () => getEarningsCalendar(watch),
    enabled: native && alphaVantage && watch.length > 0,
    staleTime: 6 * 60 * 60 * 1000,
    retry: false,
  })
  const status = useQuery({
    queryKey: ['calendar-status'],
    queryFn: calendarStatus,
    enabled: native,
  })
  const access = status.data?.access ?? 'unavailable'
  const [first, last] = [days[0], days.at(-1) ?? days[0]]
  const calendars = useQuery({
    queryKey: ['calendar-list'],
    queryFn: calendarList,
    enabled: access === 'allowed',
  })
  const shown = shownCalendars.use()
  const [menuOpen, setMenuOpen] = useState(false)
  const shownIds = calendars.data
    ? shown.filter((id) => calendars.data.some((calendar) => calendar.id === id))
    : shown
  const events = useQuery({
    queryKey: ['calendar-events', first, last, shownIds],
    queryFn: () => calendarEvents(first, last, shownIds),
    enabled: access === 'allowed' && shownIds.length > 0,
    staleTime: 60 * 1000,
  })
  const toggleCalendar = (id: string) =>
    shownCalendars.save(shown.includes(id) ? shown.filter((other) => other !== id) : [...shown, id])
  const queryClient = useQueryClient()
  const requestAccess = useMutation({
    mutationFn: requestCalendarAccess,
    onSettled: () => queryClient.invalidateQueries({ queryKey: ['calendar-status'] }),
    onError: (error) =>
      toast.error('Could not ask for Calendar access', { description: errorMessage(error) }),
  })

  const activities = useMemo(
    () => buildActivities(data, names, getExternalLogosEnabled()),
    [data, names],
  )
  const spentByDay = useMemo(() => {
    const byDay = new Map<string, ActivityItem[]>()
    for (const activity of activities) {
      const date = activity.date.slice(0, 10)
      if (date < first || date > last) continue
      byDay.set(date, [...(byDay.get(date) ?? []), activity])
    }
    return byDay
  }, [activities, first, last])
  const entries = (date: string) =>
    entriesOn(
      date,
      agenda,
      earnings.data ?? [],
      shownIds.length ? (events.data ?? []) : [],
      spentByDay.get(date) ?? [],
    )

  const select = useCallback(
    (next: string) => void navigate({ search: { day: next, month: monthOf(next) }, replace: true }),
    [navigate],
  )
  const showMonth = useCallback(
    (next: string) =>
      void navigate({ search: { month: next, day: firstOfMonth(next) }, replace: true }),
    [navigate],
  )
  const openActivity = useCallback(
    (activity: ActivityItem) => {
      // Activity opens a transaction by its own ID; anything else opens on its day.
      const date = activity.date.slice(0, 10)
      void navigate(
        activity.id.startsWith('spending:')
          ? { to: '/activities', search: { open: activity.id.slice('spending:'.length) } }
          : { to: '/activities', search: { from: date, to: date } },
      )
    },
    [navigate],
  )
  const { mutate: askAccess } = requestAccess

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'calendar.month.previous',
          title: 'Previous month',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['['], label: '[' }],
          repeatable: true,
          run: () => showMonth(shiftMonth(month, -1)),
        },
        {
          id: 'calendar.month.next',
          title: 'Next month',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: [']'], label: ']' }],
          repeatable: true,
          run: () => showMonth(shiftMonth(month, 1)),
        },
        {
          id: 'calendar.today',
          title: 'Go to today',
          search: 'calendar today now',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['t'], label: 'T' }],
          disabled: day === today,
          disabledReason: 'Already showing today',
          run: () => select(today),
        },
        ...(
          [
            ['previous', 'Previous day', 'ArrowLeft', '←', -1],
            ['next', 'Next day', 'ArrowRight', '→', 1],
            ['up', 'Same day last week', 'ArrowUp', '↑', -7],
            ['down', 'Same day next week', 'ArrowDown', '↓', 7],
          ] as const
        ).map(([id, title, key, label, by]) => ({
          id: `calendar.day.${id}`,
          title,
          group: 'View',
          scope: 'page' as const,
          bindings: [{ sequence: [key], label }],
          repeatable: true,
          run: () => select(addDays(day, by)),
        })),
        {
          id: 'calendar.day.activity',
          title: 'Show this day’s activity',
          search: 'calendar day activity transactions spending',
          group: 'Here',
          scope: 'page',
          bindings: [{ sequence: ['a'], label: 'A' }],
          run: () => void navigate({ to: '/activities', search: { from: day, to: day } }),
        },
        {
          id: 'calendar.access.request',
          title: 'Show your calendars',
          search: 'calendar events access eventkit',
          group: 'Here',
          scope: 'page',
          disabled: access !== 'notAsked' && access !== 'writeOnly',
          disabledReason:
            access === 'allowed'
              ? 'Choose calendars from the Calendars menu'
              : 'Allow Brief in System Settings → Privacy & Security → Calendars',
          run: () => askAccess(),
        },
        {
          id: 'calendar.list.toggle',
          title: 'Choose which calendars show',
          search: 'calendar calendars list choose show hide google icloud',
          group: 'Here',
          scope: 'page',
          bindings: [{ sequence: ['c'], label: 'C' }],
          disabled: access === 'unavailable',
          disabledReason: 'Your calendars show in the app',
          run: () => setMenuOpen((open) => !open),
        },
      ],
      [access, askAccess, day, month, navigate, select, showMonth, today],
    ),
  )

  const dayActivities = spentByDay.get(day) ?? []
  // Each day's and month's spending and income, as Rust summed them.
  const nothing = { spent: 0, income: 0, net: 0 }
  const totalsOn = (date: string) =>
    data.insights?.activityDays.find(({ period }) => period === date) ?? nothing
  const totals = totalsOn(day)
  const dayScheduled = entries(day).filter(({ type }) => type !== 'activity')
  const monthTotals =
    data.insights?.activityMonths.find(({ period }) => period === month) ?? nothing
  // Each day's spending against the busiest day shown, for the cells' tint.
  const spentOn = (date: string) => totalsOn(date).spent
  const busiest = Math.max(0, ...days.map(spentOn))
  const tripOn = (date: string) =>
    agenda.find(
      (item) => item.kind === 'trip' && item.date <= date && date <= (item.end ?? item.date),
    )

  return (
    <div className="page calendar-page">
      <WorkspaceHeader title="Calendar" />
      <div className="calendar-layout">
        <Card className="calendar-month-card">
          <SectionHeading
            title={monthFormatter.format(new Date(`${firstOfMonth(month)}T00:00:00Z`))}
            action={
              <>
                <CalendarsMenu
                  access={access}
                  calendars={calendars.data ?? []}
                  failed={calendars.isError}
                  shownIds={shownIds}
                  open={menuOpen}
                  onOpenChange={setMenuOpen}
                  onToggle={toggleCalendar}
                  requesting={requestAccess.isPending}
                  onRequest={() => requestAccess.mutate()}
                />
                <div className="calendar-month-actions">
                  <Button
                    size="icon-compact"
                    variant="ghost"
                    icon={ChevronRight}
                    className="calendar-previous"
                    aria-keyshortcuts="["
                    data-command="calendar.month.previous"
                    onClick={() => showMonth(shiftMonth(month, -1))}
                  >
                    Previous month
                  </Button>
                  <Button
                    size="compact"
                    variant="ghost"
                    aria-keyshortcuts="t"
                    data-command="calendar.today"
                    disabled={day === today}
                    onClick={() => select(today)}
                  >
                    Today
                  </Button>
                  <Button
                    size="icon-compact"
                    variant="ghost"
                    icon={ChevronRight}
                    aria-keyshortcuts="]"
                    data-command="calendar.month.next"
                    onClick={() => showMonth(shiftMonth(month, 1))}
                  >
                    Next month
                  </Button>
                </div>
              </>
            }
          />
          <p className="calendar-month-summary">
            <span className="sensitive-value">
              <strong className={monthTotals.spent ? 'negative' : 'muted'}>
                {formatCurrency(monthTotals.spent)}
              </strong>{' '}
              spent
            </span>
            <span className="sensitive-value">
              <strong className={monthTotals.income ? 'positive' : 'muted'}>
                {formatCurrency(monthTotals.income)}
              </strong>{' '}
              in
            </span>
          </p>
          <div
            className="calendar-grid"
            role="grid"
            aria-label="Days"
            aria-keyshortcuts="ArrowLeft ArrowRight ArrowUp ArrowDown"
          >
            <div className="calendar-weekdays" role="row">
              {weekdays.map((weekday) => (
                <span key={weekday} role="columnheader">
                  {weekday}
                </span>
              ))}
            </div>
            {Array.from({ length: 6 }, (_, week) => (
              <div className="calendar-week" role="row" key={days[week * 7]}>
                {days.slice(week * 7, week * 7 + 7).map((date) => (
                  <DayCell
                    key={date}
                    date={date}
                    outside={monthOf(date) !== month}
                    today={date === today}
                    selected={date === day}
                    spent={spentOn(date)}
                    heat={busiest ? spentOn(date) / busiest : 0}
                    trip={tripOn(date)}
                    entries={entries(date).filter(
                      (entry) => entry.type !== 'agenda' || entry.item.kind !== 'trip',
                    )}
                    accountName={accountName}
                    onSelect={() => select(date)}
                  />
                ))}
              </div>
            ))}
          </div>
          <ul className="calendar-legend" aria-label="Colors">
            {legend.map(({ label, color }) => (
              <li key={label} style={{ '--chip-color': color }}>
                {label}
              </li>
            ))}
          </ul>
        </Card>
        <div className="calendar-day-column">
          <Card className="calendar-day-card">
            <SectionHeading title={dayFormatter.format(new Date(`${day}T00:00:00Z`))} />
            <div className="calendar-day-metrics">
              <Metric
                label="Spent"
                tone={totals.spent ? 'negative' : 'muted'}
                value={<span className="sensitive-value">{formatCurrency(totals.spent)}</span>}
              />
              <Metric
                label="In"
                tone={totals.income ? 'positive' : 'muted'}
                value={<span className="sensitive-value">{formatCurrency(totals.income)}</span>}
              />
            </div>
            {dayScheduled.length ? (
              <EntryList entries={dayScheduled} accountName={accountName} />
            ) : null}
            {events.isError ? <EmptyState>Couldn’t read Calendar.</EmptyState> : null}
          </Card>
          <Card className="calendar-activity-card">
            <SectionHeading
              title="Activity"
              action={
                dayActivities.length ? (
                  <Link
                    to="/activities"
                    search={{ from: day, to: day }}
                    className="section-heading-link"
                    aria-keyshortcuts="a"
                    data-command="calendar.day.activity"
                  >
                    Open in Activity
                    <ChevronRight size={12} aria-hidden="true" />
                  </Link>
                ) : null
              }
            />
            <ActivityList
              activities={dayActivities}
              referenceIso={data.updatedAt}
              emptyMessage="No activity."
              onOpen={openActivity}
            />
          </Card>
        </div>
      </div>
    </div>
  )
}

/** The user's own calendars: ask for access, or choose which ones show beside Brief's dates. */
function CalendarsMenu({
  access,
  calendars,
  failed,
  shownIds,
  open,
  onOpenChange,
  onToggle,
  requesting,
  onRequest,
}: {
  access: CalendarAccess
  calendars: { id: string; title: string; source: string; color?: string | null }[]
  failed: boolean
  shownIds: readonly string[]
  open: boolean
  onOpenChange: (open: boolean) => void
  onToggle: (id: string) => void
  requesting: boolean
  onRequest: () => void
}) {
  const { container, positionMethod } = usePopupPortal()
  const unavailable = access === 'unavailable'
  return (
    <Popover.Root open={open && !unavailable} onOpenChange={onOpenChange}>
      <Popover.Trigger
        className="button-base button-ghost button-compact calendar-menu-trigger"
        aria-label="Your calendars"
        aria-keyshortcuts="c"
        data-command="calendar.list.toggle"
        disabled={unavailable}
        title={unavailable ? 'Your calendars show in the app.' : undefined}
      >
        <CalendarDays size={14} aria-hidden="true" />
        <span>Calendars</span>
        {shownIds.length ? <span className="calendar-menu-count">{shownIds.length}</span> : null}
        <ChevronDown size={12} aria-hidden="true" />
      </Popover.Trigger>
      <Popover.Portal container={container}>
        <Popover.Positioner
          side="bottom"
          align="end"
          sideOffset={6}
          positionMethod={positionMethod}
          className="date-range-positioner"
        >
          <Popover.Popup className="date-range-popup calendar-menu-popup" aria-label="Calendars">
            {access === 'allowed' ? (
              calendars.length ? (
                <div className="date-range-presets" role="group" aria-label="Calendars shown">
                  {calendars.map((calendar) => {
                    const on = shownIds.includes(calendar.id)
                    return (
                      <button
                        type="button"
                        key={calendar.id}
                        className="calendar-toggle"
                        aria-pressed={on}
                        data-keyboard-row
                        onClick={() => onToggle(calendar.id)}
                      >
                        <span
                          className="calendar-toggle-check"
                          style={{ '--chip-color': calendar.color ?? 'var(--text-tertiary)' }}
                          aria-hidden="true"
                        >
                          {on ? <Check size={11} /> : null}
                        </span>
                        <span className="calendar-toggle-title">{calendar.title}</span>
                        <span className="calendar-toggle-source">{calendar.source}</span>
                      </button>
                    )
                  })}
                </div>
              ) : (
                <p className="calendar-menu-note">
                  {failed ? 'Couldn’t read Calendar.' : 'No calendars.'}
                </p>
              )
            ) : access === 'notAsked' || access === 'writeOnly' ? (
              <div className="calendar-menu-note">
                <p>Add your own calendars beside Brief’s dates.</p>
                <Button
                  size="compact"
                  data-command="calendar.access.request"
                  disabled={requesting}
                  aria-busy={requesting || undefined}
                  onClick={onRequest}
                >
                  Show your calendars
                </Button>
              </div>
            ) : (
              <p className="calendar-menu-note">
                Allow Brief in System Settings → Privacy &amp; Security → Calendars.
              </p>
            )}
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  )
}

function DayCell({
  date,
  outside,
  today,
  selected,
  spent,
  heat,
  trip,
  entries,
  accountName,
  onSelect,
}: {
  date: string
  outside: boolean
  today: boolean
  selected: boolean
  spent: number
  /** The day's spending as a share of the busiest day shown, 0 to 1. */
  heat: number
  /** The trip under way that day, drawn as a band across its days. */
  trip?: AgendaItem
  entries: DayEntry[]
  accountName: (id: string) => string
  onSelect: () => void
}) {
  const shown = entries.slice(0, MAX_LINES)
  const more = entries.length - shown.length
  const weekday = new Date(`${date}T00:00:00Z`).getUTCDay()
  const tripStarts = trip && (trip.date === date || weekday === 0)
  const tripEnds = trip && ((trip.end ?? trip.date) === date || weekday === 6)
  return (
    <div
      role="gridcell"
      aria-selected={selected}
      aria-label={dayFormatter.format(new Date(`${date}T00:00:00Z`))}
      className={`calendar-day${outside ? ' is-outside' : ''}${today ? ' is-today' : ''}${selected ? ' is-selected' : ''}${weekday === 0 || weekday === 6 ? ' is-weekend' : ''}`}
      style={{ '--heat': heat.toFixed(3) }}
      onClick={onSelect}
    >
      <div className="calendar-day-top">
        <span className="calendar-day-number">{Number(date.slice(8))}</span>
        {spent ? (
          <span className="calendar-day-spent sensitive-value">{formatCurrency(spent)}</span>
        ) : null}
      </div>
      {trip ? (
        <span
          className={`calendar-band${tripStarts ? ' is-start' : ''}${tripEnds ? ' is-end' : ''}`}
          style={{ '--chip-color': kindColor.trip }}
        >
          {tripStarts ? agendaLabel(trip, accountName) : ' '}
        </span>
      ) : null}
      {shown.map((entry) => (
        <span
          key={entry.key}
          className={`calendar-chip${entry.type === 'activity' ? ' is-activity' : ''}`}
          style={{ '--chip-color': entryColor(entry) }}
        >
          <span className="calendar-chip-title">{entryTitle(entry, accountName)}</span>
          {entry.type === 'activity' ? (
            <span
              className={`calendar-chip-amount sensitive-value ${valueTone(entry.activity.amount)}`}
            >
              {formatSignedCurrency(entry.activity.amount)}
            </span>
          ) : null}
        </span>
      ))}
      {more > 0 ? <span className="calendar-more">{`${more} more`}</span> : null}
    </div>
  )
}

function entryColor(entry: DayEntry) {
  if (entry.type === 'agenda') return kindColor[entry.item.kind]
  if (entry.type === 'earnings') return kindColor.earnings
  if (entry.type === 'activity') return categoryColor(entry.activity)
  return entry.event.color ?? 'var(--text-tertiary)'
}

function entryTitle(entry: DayEntry, accountName: (id: string) => string) {
  if (entry.type === 'agenda') return agendaLabel(entry.item, accountName)
  if (entry.type === 'earnings') return `${entry.earnings.symbol} earnings`
  if (entry.type === 'activity') return formatActivityName(entry.activity.title)
  return entry.event.title || 'Busy'
}

function EntryList({
  entries,
  accountName,
}: {
  entries: DayEntry[]
  accountName: (id: string) => string
}) {
  return (
    <ul className="calendar-day-list">
      {entries.map((entry) => (
        <li key={entry.key} data-keyboard-row>
          <EntryRow entry={entry} accountName={accountName} />
        </li>
      ))}
    </ul>
  )
}

/** One dated line of the day: what it is, and the page that owns it. */
function EntryRow({
  entry,
  accountName,
}: {
  entry: DayEntry
  accountName: (id: string) => string
}) {
  if (entry.type === 'activity') return null
  const title = entryTitle(entry, accountName)
  const body = (detail: string, amount?: number) => (
    <>
      <span className="calendar-entry-bar" style={{ background: entryColor(entry) }} />
      <span className="calendar-entry-title">{title}</span>
      {amount === undefined ? null : (
        <span className={`calendar-entry-amount sensitive-value ${valueTone(amount)}`}>
          {formatSignedCurrency(amount)}
        </span>
      )}
      <span className="calendar-entry-detail">{detail}</span>
    </>
  )
  if (entry.type === 'event') {
    const { event } = entry
    const people = event.attendees.map(({ name }) => name).filter(Boolean)
    return (
      <div className="calendar-entry">
        {body(
          [
            eventTime(event) ?? 'All day',
            event.calendar,
            event.location,
            people.length ? `With ${people.join(', ')}` : null,
          ]
            .filter(Boolean)
            .join(' · '),
        )}
      </div>
    )
  }
  if (entry.type === 'earnings') {
    const { earnings } = entry
    return (
      <Link to="/holdings/$ticker" params={{ ticker: earnings.symbol }} className="calendar-entry">
        {body(
          [
            'Earnings',
            earnings.estimate === null ? null : `estimated EPS $${earnings.estimate.toFixed(2)}`,
          ]
            .filter(Boolean)
            .join(' · '),
        )}
      </Link>
    )
  }
  const { item } = entry
  const content = body(agendaDetail(item, accountName, formatCurrency), item.amount)
  if (item.ticker)
    return (
      <Link to="/holdings/$ticker" params={{ ticker: item.ticker }} className="calendar-entry">
        {content}
      </Link>
    )
  if (item.kind === 'trip')
    return (
      <Link
        to="/insights"
        // The item is `trip:<trip id>:<start>`.
        search={{ trip: item.id.slice('trip:'.length, -':0000-00-00'.length) }}
        className="calendar-entry"
      >
        {content}
      </Link>
    )
  if (item.kind === 'bill' || item.kind === 'subscription' || item.kind === 'income')
    return (
      <Link to="/spending" className="calendar-entry">
        {content}
      </Link>
    )
  return (
    <Link to="/accounts" search={{ account: item.accountId }} className="calendar-entry">
      {content}
    </Link>
  )
}
