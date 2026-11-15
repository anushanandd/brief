import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

import { summaryInputsKey, warmInvestmentSummary } from '../components/investment-summary-card'
import { isTauri } from '../lib/api'
import { sessionGraphWindow, chartPeriod } from '../lib/graph-preferences'
import type { FinanceSnapshot } from '../lib/schema'

// Long enough that launch and the first page paint first.
const WARMUP_DELAY_MS = 3_000
// Snapshots and periods already warmed this session; the period is the one in view when the
// warm-up runs, the same session chart period Investments opens on.
const warmed = new Set<string>()

/**
 * Prepares Investments before it is opened: has Rust make the portfolio's summary for the app's
 * chart period and each holding's after it, largest first, so the page and every holding open on
 * a saved summary. Rust owns everything after that: it refreshes news and filings on its own
 * schedule, makes the summaries again when something new arrives or the market's prices move on,
 * and says so. `investment-summary-updated` names a summary it made, and `data-updated` the
 * holdings whose stories or readings changed, so only their queries ask again.
 */
export function useInvestmentsWarmup(data: FinanceSnapshot | undefined, connected: boolean) {
  const client = useQueryClient()
  useEffect(() => {
    if (!isTauri()) return undefined
    let disposed = false
    const stops: Array<() => void> = []
    const keep = (unlisten: () => void) => {
      if (disposed) unlisten()
      else stops.push(unlisten)
    }
    void listen<{ start: string; ticker: string | null }>(
      'investment-summary-updated',
      ({ payload }) =>
        void client.invalidateQueries({
          queryKey: ['investment-summary', payload.start, payload.ticker ?? ''],
        }),
    ).then(keep)
    void listen<{ kind: string; symbols: string[] }>('data-updated', ({ payload }) => {
      if (payload.kind !== 'news') return
      for (const symbol of payload.symbols)
        void client.invalidateQueries({ queryKey: ['holding-news', 'alpaca', symbol] })
    }).then(keep)
    return () => {
      disposed = true
      for (const stop of stops) stop()
    }
  }, [client])
  useEffect(() => {
    if (!data || !connected || !isTauri() || !data.holdings.length) return undefined
    const timer = window.setTimeout(() => {
      const range = chartPeriod(sessionGraphWindow())
      const key = JSON.stringify([...summaryInputsKey(data), range])
      if (warmed.has(key)) return
      warmed.add(key)
      void warmInvestmentSummary(client, data, range).catch(() => undefined)
    }, WARMUP_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [client, connected, data])
}
