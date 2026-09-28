import { useMemo, useSyncExternalStore } from 'react'

import { analyticsCharts, type AnalyticsChart } from './analytics'
import { analyticsNavigation } from './navigation'

const key = 'brief.analyticsOrder'
const changed = 'brief:analytics-order'

export function parseAnalyticsOrder(raw: string | null): AnalyticsChart[] {
  let saved: unknown
  try {
    saved = raw ? JSON.parse(raw) : []
  } catch {
    saved = []
  }
  const known = analyticsCharts.map(({ id }) => id)
  const valid = Array.isArray(saved)
    ? saved.filter((id): id is AnalyticsChart => known.includes(id))
    : []
  return [...new Set([...valid, ...known])]
}

export function moveAnalyticsChart(order: AnalyticsChart[], index: number, direction: -1 | 1) {
  const next = [...order]
  const target = index + direction
  if (index >= 0 && index < next.length && target >= 0 && target < next.length)
    [next[index], next[target]] = [next[target], next[index]]
  return next
}

export function saveAnalyticsOrder(order: AnalyticsChart[]) {
  window.localStorage.setItem(key, JSON.stringify(order))
  window.dispatchEvent(new Event(changed))
}

const snapshot = () => (typeof window === 'undefined' ? null : window.localStorage.getItem(key))
const subscribe = (notify: () => void) => {
  const onStorage = (event: StorageEvent) => {
    if (event.key === key || event.key === null) notify()
  }
  window.addEventListener(changed, notify)
  window.addEventListener('storage', onStorage)
  return () => {
    window.removeEventListener(changed, notify)
    window.removeEventListener('storage', onStorage)
  }
}

export function useAnalyticsOrder() {
  const raw = useSyncExternalStore(subscribe, snapshot, () => null)
  return useMemo(() => parseAnalyticsOrder(raw), [raw])
}

export function useOrderedAnalyticsNavigation() {
  const order = useAnalyticsOrder()
  return useMemo(
    () => order.map((id) => analyticsNavigation.find(({ search }) => search.chart === id)!),
    [order],
  )
}
