import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { toast } from 'sonner'

import { useFinance } from '../../hooks/use-finance'
import { useFinanceEdits } from '../../hooks/use-finance-edits'
import { isTauri } from '../../lib/api'
import {
  calendarSources,
  calendarStatus,
  createBriefCalendar,
  removeBriefEvents,
  requestCalendarAccess,
  type CalendarStatus,
} from '../../lib/calendar'
import { errorMessage } from '../../lib/errors'
import { financeSettings } from '../../lib/finance-settings'
import { formatUpdatedAt } from '../../lib/format'
import { Trash2, X } from '../icons'
import { SettingsCard, SettingsRow, SettingsSwitch } from '../settings-ui'
import { Button, ChartRangeSelect } from '../ui'

function describe(status: CalendarStatus | undefined, enabled: boolean) {
  if (!status || status.access === 'unavailable') return 'Available in the app.'
  if (status.access === 'denied' || status.access === 'restricted')
    return 'Allow Brief in System Settings → Privacy & Security → Calendars.'
  if (!enabled)
    return 'Payments due, statements, bills, income, credits, and trips, with amounts and accounts, in your calendar named Brief.'
  if (status.missing) return 'Deleted in Calendar. Turn this off and on to make it again.'
  const where = status.calendar ? `In ${status.calendar.source}` : 'Brief calendar'
  if (status.lastSync?.error) return `${where}. ${status.lastSync.error}.`
  return status.lastSync ? `${where}. Updated ${formatUpdatedAt(status.lastSync.at)}.` : `${where}.`
}

/**
 * A "Brief" calendar in macOS Calendar that Rust keeps matched to the agenda. Turning it on
 * asks for Calendar access, then uses a calendar already named "Brief" or makes one in the
 * chosen account; turning it off can keep Brief's events or remove them.
 */
export function CalendarSyncCard() {
  const finance = useFinance()
  const { updateSettings } = useFinanceEdits()
  const queryClient = useQueryClient()
  const settings = financeSettings(finance.data)
  const enabled = settings.calendarSync
  const native = isTauri()
  const status = useQuery({
    queryKey: ['calendar-status'],
    queryFn: calendarStatus,
    enabled: native,
  })
  const allowed = status.data?.access === 'allowed'
  const needsSource = allowed && !enabled
  const sources = useQuery({
    queryKey: ['calendar-sources'],
    queryFn: calendarSources,
    enabled: needsSource,
  })
  const [sourceId, setSourceId] = useState<string>()
  const chosen = sourceId ?? sources.data?.[0]?.id
  const [pending, setPending] = useState<'on' | 'keep' | 'remove' | null>(null)
  const [confirmingOff, setConfirmingOff] = useState(false)

  const run = async (task: typeof pending, failure: string, work: () => Promise<unknown>) => {
    setPending(task)
    try {
      await work()
    } catch (error) {
      toast.error(failure, { description: errorMessage(error) })
    } finally {
      setPending(null)
      void queryClient.invalidateQueries({ queryKey: ['calendar-status'] })
      void queryClient.invalidateQueries({ queryKey: ['calendar-events'] })
    }
  }
  const turnOn = () =>
    run('on', 'Could not make the Brief calendar', async () => {
      let access = status.data?.access
      if (access !== 'allowed') access = await requestCalendarAccess()
      if (access !== 'allowed') return
      const source = chosen ?? (await calendarSources())[0]?.id
      if (!source) throw new Error('No calendar account can hold a new calendar')
      const calendar = await createBriefCalendar(source)
      await updateSettings((current) => ({
        ...current,
        calendarSync: true,
        calendarId: calendar.id,
      }))
    })
  const turnOff = (remove: boolean) =>
    run(remove ? 'remove' : 'keep', 'Could not turn off the Brief calendar', async () => {
      if (remove) await removeBriefEvents()
      await updateSettings((current) => ({ ...current, calendarSync: false }))
      setConfirmingOff(false)
    })

  const blocked =
    !native ||
    Boolean(finance.data?.recovery) ||
    status.data?.access === 'denied' ||
    status.data?.access === 'restricted'
  return (
    <SettingsCard>
      <SettingsRow
        id="brief-calendar"
        title="Brief calendar"
        description={describe(status.data, enabled)}
      >
        <SettingsSwitch
          labelledBy="brief-calendar-title"
          describedBy="brief-calendar-description"
          checked={enabled}
          disabled={blocked || pending !== null || confirmingOff}
          onCheckedChange={(checked) => (checked ? void turnOn() : setConfirmingOff(true))}
        />
      </SettingsRow>
      {needsSource && (sources.data?.length ?? 0) > 1 && chosen ? (
        <SettingsRow
          id="brief-calendar-account"
          title="Calendar account"
          description="Where Brief makes its calendar if you have none named Brief."
        >
          <ChartRangeSelect
            label="Calendar account"
            options={(sources.data ?? []).map(({ id, title }) => ({
              value: id,
              label: title,
              accessibleLabel: title,
            }))}
            value={chosen}
            onValueChange={setSourceId}
          />
        </SettingsRow>
      ) : null}
      {confirmingOff ? (
        <div
          className="connection-confirmation"
          role="group"
          aria-label="Turn off the Brief calendar"
        >
          <p>Keep Brief’s events in the calendar, or remove them?</p>
          <div>
            <Button
              icon={X}
              size="compact"
              disabled={pending !== null}
              onClick={() => setConfirmingOff(false)}
            >
              Cancel
            </Button>
            <Button size="compact" disabled={pending !== null} onClick={() => void turnOff(false)}>
              {pending === 'keep' ? 'Turning off…' : 'Keep events'}
            </Button>
            <Button
              icon={Trash2}
              size="compact"
              variant="destructive"
              disabled={pending !== null || !allowed}
              onClick={() => void turnOff(true)}
            >
              {pending === 'remove' ? 'Removing…' : 'Remove events'}
            </Button>
          </div>
        </div>
      ) : null}
    </SettingsCard>
  )
}
