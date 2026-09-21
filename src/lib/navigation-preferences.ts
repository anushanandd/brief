import { useMemo, useSyncExternalStore } from 'react'

import { navigation } from './navigation'

const key = 'brief.pageOrder'
const changed = 'brief:page-order'
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
export function savePageOrder(order: PagePath[]) {
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
export function useOrderedNavigation() {
  const raw = useSyncExternalStore(subscribe, snapshot, () => null)
  return useMemo(
    () =>
      parsePageOrder(raw).map((path, index) => ({
        ...navigation.find(({ to }) => to === path)!,
        shortcut: String(index + 1),
      })),
    [raw],
  )
}
export function adjacentPage(order: PagePath[], pathname: string, direction: -1 | 1) {
  const current = order.findIndex((path) =>
    path === '/' ? pathname === '/' : pathname === path || pathname.startsWith(`${path}/`),
  )
  const index = current < 0 ? order.indexOf('/settings') : current
  return order[(index + direction + order.length) % order.length]
}
