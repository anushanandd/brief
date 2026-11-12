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
  const [moving, displaced] = [next[index], next[target]]
  if (moving !== undefined && displaced !== undefined) {
    next[index] = displaced
    next[target] = moving
  }
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
