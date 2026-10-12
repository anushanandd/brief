import { useQuery } from '@tanstack/react-query'
import { useMemo } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import { getEarningsCalendar, isTauri } from '../lib/api'
import { RefreshCw } from './icons'
import { Button } from './ui'

const dateFormat = new Intl.DateTimeFormat('en-US', { month: 'short', day: 'numeric' })

export function HoldingEarnings({ symbols, connected }: { symbols: string[]; connected: boolean }) {
  const native = typeof window !== 'undefined' && isTauri()
  const earnings = useQuery({
    queryKey: ['earnings-calendar', symbols],
    queryFn: () => getEarningsCalendar(symbols),
    enabled: native && connected && symbols.length > 0,
    staleTime: 6 * 60 * 60 * 1000,
    retry: false,
  })
  const failed = native && connected && earnings.isError
  const { refetch } = earnings
  useRegisterActions(
    useMemo<AppAction[]>(
      () =>
        failed
          ? [
              {
                id: 'holding.earnings.retry',
                title: 'Retry earnings date',
                search: 'earnings calendar retry',
                group: 'Actions',
                scope: 'page',
                run: () => void refetch(),
              },
            ]
          : [],
      [failed, refetch],
    ),
  )
  return (
    <div className="metric holding-earnings">
      <span>Next earnings</span>
      {native && connected && earnings.isLoading ? (
        <strong className="muted" role="status">
          Loading…
        </strong>
      ) : native && connected && earnings.isError ? (
        <Button
          icon={RefreshCw}
          size="compact"
          onClick={() => void earnings.refetch()}
          aria-label="Retry earnings date"
          data-command="holding.earnings.retry"
        >
          Retry
        </Button>
      ) : earnings.data?.length ? (
        <strong className="holding-earnings-dates">
          {earnings.data.map((event) => (
            <time key={`${event.symbol}:${event.reportDate}`} dateTime={event.reportDate}>
              {dateFormat.format(new Date(`${event.reportDate}T12:00:00`))}
            </time>
          ))}
        </strong>
      ) : (
        <strong className="muted" aria-label="Next earnings date unavailable">
          —
        </strong>
      )}
    </div>
  )
}
