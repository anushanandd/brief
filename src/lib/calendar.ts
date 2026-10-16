import { invoke } from '@tauri-apps/api/core'
import { z } from 'zod'

import type { ActivityItem } from './activity'
import { isTauri } from './api'
import { addDays, validDate } from './dates'
import { displayCase } from './format'
import { preference } from './preference'
import type { AgendaItem, EarningsEvent } from './schema'

/**
 * macOS Calendar: Brief's own "Brief" calendar, which Rust keeps matched to the agenda, and the
 * user's other events, read for this page. Only the app can reach Calendar; the browser
 * preview reports it unavailable and shows the agenda alone.
 */
export const calendarAccessSchema = z.enum([
  'notAsked',
  'restricted',
  'denied',
  'allowed',
  'writeOnly',
  'unavailable',
])
export type CalendarAccess = z.infer<typeof calendarAccessSchema>

const calendarInfoSchema = z.object({ id: z.string(), title: z.string(), source: z.string() })
export type CalendarInfo = z.infer<typeof calendarInfoSchema>

const calendarStatusSchema = z.object({
  access: calendarAccessSchema,
  calendar: calendarInfoSchema.optional(),
  /** Sync is on but its calendar was deleted in Calendar. */
  missing: z.boolean(),
  lastSync: z.object({ at: z.string(), error: z.string().optional() }).optional(),
})
export type CalendarStatus = z.infer<typeof calendarStatusSchema>

const calendarSourceSchema = z.object({
  id: z.string(),
  title: z.string(),
  kind: z.enum(['icloud', 'local', 'caldav', 'exchange', 'other']),
})
export type CalendarSource = z.infer<typeof calendarSourceSchema>

const calendarEventSchema = z.object({
  id: z.string(),
  calendar: z.string(),
  title: z.string(),
  /** A date for an all-day event, otherwise a local instant. */
  start: z.string(),
  end: z.string(),
  allDay: z.boolean(),
  color: z.string().optional(),
  location: z.string().optional(),
  attendees: z.array(z.object({ name: z.string(), contactId: z.string().optional() })).default([]),
})
export type CalendarEvent = z.infer<typeof calendarEventSchema>

const unavailableStatus: CalendarStatus = { access: 'unavailable', missing: false }

export async function calendarStatus(): Promise<CalendarStatus> {
  if (!isTauri()) return unavailableStatus
  return calendarStatusSchema.parse(await invoke('calendar_status'))
}

/** Shows macOS's prompt the first time; afterward macOS answers from the saved choice. */
export async function requestCalendarAccess(): Promise<CalendarAccess> {
  if (!isTauri()) return 'unavailable'
  return calendarAccessSchema.parse(await invoke('request_calendar_access'))
}

export async function calendarSources(): Promise<CalendarSource[]> {
  if (!isTauri()) return []
  return calendarSourceSchema.array().parse(await invoke('calendar_sources'))
}

/**
 * The Brief calendar: the saved one when it still exists, else one already named "Brief",
 * else a new one in `sourceId`.
 */
export async function createBriefCalendar(sourceId: string): Promise<CalendarInfo> {
  return calendarInfoSchema.parse(await invoke('create_brief_calendar', { sourceId }))
}

/** Removes the events Brief wrote; the calendar and the user's own events stay. */
export async function removeBriefEvents(): Promise<void> {
  await invoke('remove_brief_events')
}

const listedCalendarSchema = z.object({
  id: z.string(),
  title: z.string(),
  source: z.string(),
  color: z.string().optional(),
})
export type ListedCalendar = z.infer<typeof listedCalendarSchema>

/** The user's calendars besides the Brief calendar. */
export async function calendarList(): Promise<ListedCalendar[]> {
  if (!isTauri()) return []
  return listedCalendarSchema.array().parse(await invoke('calendar_list'))
}

/** The user's events in `calendarIds` from `start` through `end`. */
export async function calendarEvents(
  start: string,
  end: string,
  calendarIds: readonly string[],
): Promise<CalendarEvent[]> {
  if (!isTauri() || !calendarIds.length) return []
  return calendarEventSchema
    .array()
    .parse(await invoke('calendar_events', { start, end, calendarIds }))
}

/**
 * The calendars the Calendar page shows beside Brief's items and activity: none until the user
 * picks some. A display choice, so it stays in this window's storage.
 */
export const shownCalendars = preference<readonly string[]>(
  'brief.calendar.shown',
  (raw) => {
    try {
      const parsed: unknown = JSON.parse(raw ?? '[]')
      return Array.isArray(parsed) ? parsed.filter((id) => typeof id === 'string') : []
    } catch {
      return []
    }
  },
  (ids) => JSON.stringify(ids),
)

/** The Calendar page's search: the month shown and the day picked. A `brief://calendar`
 * link from an event in the Brief calendar names the day and its item. */
export type CalendarSearch = { month?: string; day?: string }

export function calendarSearch(search: Record<string, unknown>): CalendarSearch {
  const day = validDate(search.day)
  return {
    ...(typeof search.month === 'string' && /^\d{4}-(0[1-9]|1[0-2])$/.test(search.month)
      ? { month: search.month }
      : {}),
    ...(day ? { day } : {}),
  }
}

/** `2026-10` from a date, and back to the month's first day. */
export const monthOf = (date: string) => date.slice(0, 7)
export const firstOfMonth = (month: string) => `${month}-01`

export function shiftMonth(month: string, by: number) {
  const [year, index] = month.split('-').map(Number)
  const shifted = new Date(Date.UTC(year, index - 1 + by, 1))
  return shifted.toISOString().slice(0, 7)
}

/** The six weeks shown for a month, Sunday first, as dates. */
export function monthGrid(month: string): string[] {
  const first = firstOfMonth(month)
  const weekday = new Date(`${first}T00:00:00Z`).getUTCDay()
  const start = addDays(first, -weekday)
  return Array.from({ length: 42 }, (_, index) => addDays(start, index))
}

/** The days an item or event covers, inclusive. */
const covers = (start: string, end: string, day: string) => start <= day && day <= end

/** One line on a day: an agenda item, an earnings date, or a calendar event. */
export type DayEntry =
  | { type: 'agenda'; key: string; item: AgendaItem }
  | { type: 'earnings'; key: string; earnings: EarningsEvent }
  | { type: 'event'; key: string; event: CalendarEvent }
  | { type: 'activity'; key: string; activity: ActivityItem }

/** Brief's items first, by the agenda's order, then events by start, then the day's activity. */
export function entriesOn(
  day: string,
  agenda: readonly AgendaItem[],
  earnings: readonly EarningsEvent[],
  events: readonly CalendarEvent[],
  activities: readonly ActivityItem[] = [],
): DayEntry[] {
  return [
    ...agenda
      .filter((item) => covers(item.date, item.end ?? item.date, day))
      .map((item) => ({ type: 'agenda' as const, key: item.id, item })),
    ...earnings
      .filter(({ reportDate }) => reportDate === day)
      .map((report) => ({
        type: 'earnings' as const,
        key: `earnings:${report.symbol}:${day}`,
        earnings: report,
      })),
    ...events
      .filter((event) => covers(event.start.slice(0, 10), event.end.slice(0, 10), day))
      .toSorted((left, right) => left.start.localeCompare(right.start))
      .map((event) => ({ type: 'event' as const, key: `event:${event.id}:${day}`, event })),
    ...activities
      .filter((activity) => activity.date.slice(0, 10) === day)
      .map((activity) => ({ type: 'activity' as const, key: activity.id, activity })),
  ]
}

/**
 * What else an agenda item knows, for the day's list: its account, how often it repeats, the
 * range it has posted in, and whether Brief estimated it.
 */
export function agendaDetail(
  item: AgendaItem,
  accountName: (id: string) => string,
  formatAmount: (value: number) => string,
) {
  const range =
    item.amountLow !== undefined && item.amountHigh !== undefined
      ? `${formatAmount(Math.min(Math.abs(item.amountLow), Math.abs(item.amountHigh)))}–${formatAmount(Math.max(Math.abs(item.amountLow), Math.abs(item.amountHigh)))}`
      : null
  return [
    agendaKindLabel[item.kind],
    item.accountId ? accountName(item.accountId) : null,
    item.frequency ?? null,
    range,
    item.minimumPayment !== undefined ? `${formatAmount(item.minimumPayment)} minimum` : null,
    item.estimated ? 'estimated' : null,
  ]
    .filter(Boolean)
    .join(' · ')
}

/** What an agenda item says, naming accounts as the user named them. */
export function agendaLabel(item: AgendaItem, accountName: (id: string) => string) {
  const account = item.accountId ? accountName(item.accountId) : 'Card'
  const title = displayCase(item.title ?? '')
  const ticker = item.ticker ?? ''
  switch (item.kind) {
    case 'paymentDue':
      return `${account} payment due`
    case 'statementCloses':
      return `${account} statement closes`
    case 'creditExpiring':
      return `${title} credit ends`
    case 'bill':
      return `${title} due`
    case 'subscription':
      return `${title} renews`
    case 'income':
      return `${title} expected`
    case 'dividend':
      return `${ticker} dividend`
    case 'longTerm':
      return `${ticker} shares turn long-term`
    case 'washSaleEnds':
      return `${ticker} wash-sale window ends`
    default:
      return `Trip: ${title}`
  }
}

/** A short kind for the day's list. */
export const agendaKindLabel: Record<AgendaItem['kind'], string> = {
  paymentDue: 'Payment',
  statementCloses: 'Statement',
  creditExpiring: 'Card credit',
  bill: 'Bill',
  subscription: 'Subscription',
  income: 'Income',
  dividend: 'Dividend',
  longTerm: 'Tax lot',
  washSaleEnds: 'Wash sale',
  trip: 'Trip',
}

const sameName = (left: string, right: string) =>
  left.trim().toLocaleLowerCase() === right.trim().toLocaleLowerCase()

/**
 * People the user already keeps who were invited to these events and aren't in `chosen`: by
 * their Contacts card, or else by the same name.
 */
export function attendingPeople<P extends { id: string; name: string; contactId?: string }>(
  events: readonly CalendarEvent[],
  people: readonly P[],
  chosen: readonly string[],
): P[] {
  const attendees = events.flatMap((event) => event.attendees)
  return people.filter(
    (person) =>
      !chosen.includes(person.id) &&
      attendees.some((attendee) =>
        person.contactId && attendee.contactId
          ? person.contactId === attendee.contactId
          : sameName(person.name, attendee.name),
      ),
  )
}

/** A timed event's local time, `9:30 AM`; all-day events have none. */
export function eventTime(event: CalendarEvent) {
  if (event.allDay) return null
  const time = new Date(event.start)
  return Number.isNaN(time.getTime())
    ? null
    : time.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' })
}
