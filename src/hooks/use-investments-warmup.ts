import { useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'

import { warmInvestmentSummary } from '../components/investment-summary-card'
import { isTauri, refreshPortfolioNews } from '../lib/api'
import { sessionGraphWindow, chartPeriod } from '../lib/graph-preferences'
import type { FinanceSnapshot } from '../lib/schema'

// Long enough that launch and the first page paint first.
const WARMUP_DELAY_MS = 3_000
// Revisions and periods already warmed this session; the period is the one in view when the
// warm-up runs.
const warmed = new Set<string>()

/**
 * Prepares Investments before it is opened: saves every holding's news (at most every ten
 * minutes, in one request), then builds the portfolio's summary for the app's chart period,
 * on-device text included, so the page opens with it instead of waiting.
 */
export function useInvestmentsWarmup(data: FinanceSnapshot | undefined, connected: boolean) {
  const client = useQueryClient()
  const revision = data?.revision
  useEffect(() => {
    if (!data || !connected || !isTauri() || !data.holdings.length) return undefined
    const timer = window.setTimeout(() => {
      const range = chartPeriod(sessionGraphWindow())
      const key = `${revision}:${range}`
      if (warmed.has(key)) return
      warmed.add(key)
      void (async () => {
        try {
          const news = await refreshPortfolioNews()
          if (news.refreshed) {
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
  }, [client, connected, data, revision])
}
