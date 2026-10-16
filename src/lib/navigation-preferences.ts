import { useMemo } from 'react'

import { navigation } from './navigation'
import { preference } from './preference'

export type PagePath = (typeof navigation)[number]['to']

export function parsePageOrder(raw: string | null): PagePath[] {
  let saved: unknown
  try {
    saved = raw ? JSON.parse(raw) : []
  } catch {
    saved = []
  }
  const known = navigation.map(({ to }) => to)
  const valid = Array.isArray(saved)
    ? saved.filter((path): path is PagePath => known.includes(path))
    : []
  return [...new Set([...valid, ...known])]
}
export function movePage(order: PagePath[], index: number, direction: -1 | 1) {
  const next = [...order]
  const target = index + direction
  if (index >= 0 && index < next.length && target >= 0 && target < next.length)
    [next[index], next[target]] = [next[target], next[index]]
  return next
}
const pageOrder = preference('brief.pageOrder', parsePageOrder, JSON.stringify)
export const savePageOrder = pageOrder.save
export function useOrderedNavigation() {
  const order = pageOrder.use()
  return useMemo(
    () =>
      order.map((path, index) => ({
        ...navigation.find(({ to }) => to === path)!,
        shortcut: `⌘${index + 1}`,
      })),
    [order],
  )
}

/** Whether the sidebar shows its reports. */
export const reportsGroupOpen = preference('brief:sidebar-analytics-open', (raw) => raw === 'true')
export function adjacentPage(order: PagePath[], pathname: string, direction: -1 | 1) {
  const current = order.findIndex((path) =>
    path === '/' ? pathname === '/' : pathname === path || pathname.startsWith(`${path}/`),
  )
  const index = current < 0 ? (direction > 0 ? -1 : 0) : current
  return order[(index + direction + order.length) % order.length]
}
