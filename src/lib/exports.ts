import { invoke } from '@tauri-apps/api/core'

import { accountDisplayName } from './account-names'
import { isTauri } from './api'
import { agendaLabel } from './calendar'
import type { FinanceSnapshot } from './schema'

/** One line of an export file, quoted for a spreadsheet; provider text can never run as a formula. */
const csvRow = (values: readonly (string | number | boolean | null | undefined)[]) =>
  values
    .map((value) => {
      let text = value == null ? '' : String(value)
      if (typeof value === 'string' && /^[\s]*[=+\-@\t\r]/.test(text)) text = `'${text}`
      return `"${text.replaceAll('"', '""')}"`
    })
    .join(',')

/**
 * Open tax lots as Rust estimated them, one row per lot, with the position's coverage so a
 * partial position is never mistaken for a complete one.
 */
export function lotsCsv(data: FinanceSnapshot) {
  const names = data.settings?.accountNames ?? {}
  const accountName = (id: string) =>
    accountDisplayName(id, data.accounts.find((account) => account.id === id)?.name ?? id, names)
  const rows = (data.insights?.lots ?? []).flatMap((position) =>
    position.lots.map((lot) =>
      csvRow([
        accountName(position.accountId),
        position.ticker,
        lot.acquired,
        lot.units,
        lot.cost,
        lot.value,
        lot.gain,
        lot.longTerm ? 'long' : 'short',
        lot.longTermOn,
        lot.estimatedTax,
        position.taxDeferred,
        position.coverage,
        position.untrackedUnits || '',
      ]),
    ),
  )
  return (
    [
      csvRow([
        'Account',
        'Ticker',
        'Acquired',
        'Units',
        'Cost',
        'Value',
        'Gain',
        'Term',
        'Long-term on',
        'Estimated tax if sold',
        'Tax deferred',
        'Coverage',
        'Untracked units',
      ]),
      ...rows,
    ].join('\r\n') + '\r\n'
  )
}

const icsText = (value: string) =>
  value
    .replaceAll('\\', '\\\\')
    .replaceAll(';', '\\;')
    .replaceAll(',', '\\,')
    .replaceAll('\n', '\\n')
const icsDate = (date: string) => date.replaceAll('-', '')

type CalendarEvent = { id: string; date: string; summary: string; description?: string }

/**
 * The agenda's dates from today on, trips aside. All-day events, with no amounts, so the file is
 * safe to put in a shared calendar.
 */
export function upcomingCalendarEvents(data: FinanceSnapshot): CalendarEvent[] {
  const insights = data.insights
  if (!insights) return []
  const names = data.settings?.accountNames ?? {}
  const accountName = (id: string) =>
    accountDisplayName(
      id,
      data.accounts.find((account) => account.id === id)?.name ?? 'Card',
      names,
    )
  return insights.agenda
    .filter((item) => item.kind !== 'trip' && item.date >= insights.today)
    .map((item) => ({
      id: item.id,
      date: item.date,
      summary: agendaLabel(item, accountName),
      ...(item.estimated ? { description: 'Estimated by Brief from past activity.' } : {}),
    }))
}

export function upcomingCalendar(data: FinanceSnapshot, stamp = new Date()) {
  const dtstamp = `${stamp.toISOString().replace(/[-:]/g, '').slice(0, 15)}Z`
  const lines = [
    'BEGIN:VCALENDAR',
    'VERSION:2.0',
    'PRODID:-//Brief//Upcoming//EN',
    'CALSCALE:GREGORIAN',
    'X-WR-CALNAME:Brief',
  ]
  for (const event of upcomingCalendarEvents(data)) {
    lines.push(
      'BEGIN:VEVENT',
      `UID:${icsText(event.id)}@brief.local`,
      `DTSTAMP:${dtstamp}`,
      `DTSTART;VALUE=DATE:${icsDate(event.date)}`,
      `SUMMARY:${icsText(event.summary)}`,
      ...(event.description ? [`DESCRIPTION:${icsText(event.description)}`] : []),
      'END:VEVENT',
    )
  }
  lines.push('END:VCALENDAR')
  return lines.join('\r\n') + '\r\n'
}

/** Saves an export through the native file picker; the browser preview downloads it. */
export async function saveExport(contents: string, filename: string, type: string) {
  if (isTauri()) return invoke<boolean>('export_finance_csv', { contents, filename })
  const url = URL.createObjectURL(new Blob([contents], { type }))
  const anchor = document.createElement('a')
  anchor.href = url
  anchor.download = filename
  anchor.click()
  setTimeout(() => URL.revokeObjectURL(url), 1000)
  return true
}

export const exportLotsCsv = (data: FinanceSnapshot) =>
  saveExport(lotsCsv(data), 'Brief-lots.csv', 'text/csv;charset=utf-8')

export const exportUpcomingCalendar = (data: FinanceSnapshot) =>
  saveExport(upcomingCalendar(data), 'Brief-upcoming.ics', 'text/calendar;charset=utf-8')
