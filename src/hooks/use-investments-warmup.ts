import { useQueryClient } from '@tanstack/react-query'
import { listen } from '@tauri-apps/api/event'
import { useEffect } from 'react'

import { summaryInputsKey, warmInvestmentSummary } from '../components/investment-summary-card'
import { isTauri, refreshPortfolioNews } from '../lib/api'
import { sessionGraphWindow, chartPeriod } from '../lib/graph-preferences'
import type { FinanceSnapshot } from '../lib/schema'

// Long enough that launch and the first page paint first.
const WARMUP_DELAY_MS = 3_000
// Snapshots and periods already warmed this session; the period is the one in view when the
// warm-up runs, the same session chart period Investments opens on.
const warmed = new Set<string>()

/**
 * Prepares Investments before it is opened: saves every holding's news (at most every ten
 * minutes, in one request), then has Rust make the portfolio's summary for the app's chart
 * period and each holding's after it, largest first, saving each, so the page and every
 * holding open on a saved summary instead of waiting. Rust says when it made one
 * (`investment-summary-updated`), and that summary's queries ask again.
 */
export function useInvestmentsWarmup(data: FinanceSnapshot | undefined, connected: boolean) {
  const client = useQueryClient()
  useEffect(() => {
    if (!isTauri()) return undefined
    let disposed = false
    let stop: (() => void) | undefined
    void listen<{ start: string; ticker: string | null }>(
      'investment-summary-updated',
      ({ payload }) =>
        void client.invalidateQueries({
          queryKey: ['investment-summary', payload.start, payload.ticker ?? ''],
        }),
    ).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })
    return () => {
      disposed = true
      stop?.()
    }
  }, [client])
  useEffect(() => {
    if (!data || !connected || !isTauri() || !data.holdings.length) return undefined
    const timer = window.setTimeout(() => {
      const range = chartPeriod(sessionGraphWindow())
      const key = JSON.stringify([...summaryInputsKey(data), range])
      if (warmed.has(key)) return
      warmed.add(key)
      void (async () => {
        try {
          const news = await refreshPortfolioNews()
          if (news.refreshed) {
            // Rust answers with the saved summaries, marked while it makes new ones.
            await client.invalidateQueries({ queryKey: ['investment-summary'] })
            void client.invalidateQueries({ queryKey: ['holding-news'] })
          }
        } catch {
          // News is optional; the summary still builds from what's saved.
        }
        await warmInvestmentSummary(client, data, range).catch(() => undefined)
      })()
    }, WARMUP_DELAY_MS)
    return () => window.clearTimeout(timer)
  }, [client, connected, data])
}
