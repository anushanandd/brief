import { useQuery } from '@tanstack/react-query'

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
  return (
    <section className="holding-earnings" aria-label="Earnings date">
      <h3>Earnings date</h3>
      {native && connected && earnings.isLoading ? (
        <span className="muted" role="status">
          Loading…
        </span>
      ) : native && connected && earnings.isError ? (
        <Button
          icon={RefreshCw}
          size="compact"
          onClick={() => void earnings.refetch()}
          aria-label="Retry earnings date"
        >
          Retry
        </Button>
      ) : earnings.data?.length ? (
        <span className="holding-earnings-dates">
          {earnings.data.map((event) => (
            <time key={`${event.symbol}:${event.reportDate}`} dateTime={event.reportDate}>
              {dateFormat.format(new Date(`${event.reportDate}T12:00:00`))}
            </time>
          ))}
        </span>
      ) : (
        <span className="muted">—</span>
      )}
    </section>
  )
}
