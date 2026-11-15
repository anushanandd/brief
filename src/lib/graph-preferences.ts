import { useSyncExternalStore, type Dispatch, type SetStateAction } from 'react'

import { addDays } from './dates'
import { preference } from './preference'

const DAY_SECONDS = 24 * 60 * 60
const DEFAULT_GRAPH_WINDOW = 7 * DAY_SECONDS

/** The chart periods every D/W/M/Q/Y/A chart offers, each one of the windows Rust computes. */
export const graphWindows = [
  { name: 'day', label: 'D', settingsLabel: 'Day', secs: DAY_SECONDS },
  { name: 'week', label: 'W', settingsLabel: 'Week', secs: 7 * DAY_SECONDS },
  { name: 'month', label: 'M', settingsLabel: 'Month', secs: 30 * DAY_SECONDS },
  { name: 'quarter', label: 'Q', settingsLabel: 'Quarter', secs: 90 * DAY_SECONDS },
  { name: 'year', label: 'Y', settingsLabel: 'Year', secs: 365 * DAY_SECONDS },
  { name: 'all', label: 'A', settingsLabel: 'All time', secs: 0 },
] as const
export type ChartPeriod = (typeof graphWindows)[number]['name']
export const chartPeriods = graphWindows.map(({ name }) => name)

/** The period a chart window in seconds shows; an unknown one shows the week. */
export const chartPeriod = (seconds: number): ChartPeriod =>
  graphWindows.find(({ secs }) => secs === seconds)?.name ?? 'week'

/** A period's chart window in seconds. */
export const chartPeriodSeconds = (period: ChartPeriod) =>
  graphWindows.find(({ name }) => name === period)!.secs

export function graphWindowForKey(key: string) {
  return graphWindows[['d', 'w', 'm', 'q', 'y', 'a'].indexOf(key.toLocaleLowerCase())]?.secs
}

export function adjacentGraphWindow(current: number, direction: -1 | 1) {
  const index = graphWindows.findIndex(({ secs }) => secs === current)
  const next = (Math.max(0, index) + direction + graphWindows.length) % graphWindows.length
  return graphWindows[next]?.secs ?? current
}

export function parseGraphWindow(value: unknown) {
  if (value === null || value === undefined || value === '') return DEFAULT_GRAPH_WINDOW
  const seconds = Number(value)
  return graphWindows.find((window) => window.secs === seconds)?.secs ?? DEFAULT_GRAPH_WINDOW
}

// The range last chosen, so the next launch opens on it.
const savedWindow = preference('brief.defaultGraphWindow', parseGraphWindow)

// One range for this session, shared by every page with a D/W/M/Q/Y/A chart, so moving between
// pages keeps the period in view. It starts from the range last chosen.
let sessionWindow: number | undefined
const listeners = new Set<() => void>()
const currentWindow = () => sessionWindow ?? savedWindow.get()
const setSessionWindow: Dispatch<SetStateAction<number>> = (next) => {
  const value = parseGraphWindow(typeof next === 'function' ? next(currentWindow()) : next)
  if (value === currentWindow()) return
  sessionWindow = value
  if (!scrollPeriodDays(value)) sessionEnd = undefined
  if (typeof window !== 'undefined') savedWindow.save(value)
  listeners.forEach((notify) => notify())
}
const subscribeWindow = (notify: () => void) => {
  listeners.add(notify)
  return () => {
    listeners.delete(notify)
  }
}

/** The session's chart period now, for work outside React that follows it. */
export const sessionGraphWindow = () => currentWindow()

export function useGraphWindow() {
  return [
    useSyncExternalStore(subscribeWindow, currentWindow, () => DEFAULT_GRAPH_WINDOW),
    setSessionWindow,
  ] as const
}

/** The range chosen this session, outside React: what a route loads before its page renders. */
export const chosenGraphWindow = () => sessionWindow

/** The range chosen this session, or undefined until one is; a page with its own default uses it. */
export function useChosenGraphWindow() {
  return useSyncExternalStore(
    subscribeWindow,
    () => sessionWindow,
    () => undefined,
  )
}

/** For tests: forget the session range. */
export function resetSessionGraphWindow() {
  sessionWindow = undefined
  sessionEnd = undefined
}

const periodDays: Partial<Record<ChartPeriod, number>> = {
  week: 7,
  month: 30,
  quarter: 90,
  year: 365,
}

/** What a chart needs to scroll back in time: the latest day, and the first it draws. */
export type GraphScroll = { today: string; first: string | undefined }

/** The days a scrolling range moves by a period; none for a day or all time, which don't scroll. */
export const scrollPeriodDays = (seconds: number) => periodDays[chartPeriod(seconds)]

/**
 * The day a page's charts end on, while scrolled back in time; undefined while they end today.
 * A range that doesn't scroll ends today again, and so does every page newly opened.
 */
let sessionEnd: string | undefined
const currentEnd = () => sessionEnd

/** The page's end day now, outside React. */
export const sessionGraphEnd = () => sessionEnd

export function setGraphEnd(end: string | undefined) {
  if (end === sessionEnd) return
  sessionEnd = end
  endMovedAt = Date.now()
  listeners.forEach((notify) => notify())
}

// When the end day last moved: figures that follow a scroll roll to their new values for a
// moment after, and otherwise update in place.
let endMovedAt = 0
const ROLLING_MS = 1_000

/** Whether the page is being scrolled through time, so its figures roll as they change. */
export function useGraphScrolling() {
  useGraphEnd()
  return Date.now() - endMovedAt < ROLLING_MS
}

export function useGraphEnd() {
  return [useSyncExternalStore(subscribeWindow, currentEnd, () => undefined), setGraphEnd] as const
}

/**
 * The end `days` later (earlier when negative), within the history: no later than `today`,
 * which is the live end, and no earlier than a full period after `first`.
 */
export function shiftedGraphEnd(
  end: string | undefined,
  days: number,
  seconds: number,
  today: string,
  first: string | undefined,
): string | undefined {
  const period = scrollPeriodDays(seconds)
  if (!period || !first) return undefined
  const earliest = addDays(first, period)
  if (earliest >= today) return undefined
  const next = addDays(end ?? today, days)
  if (next >= today) return undefined
  return next < earliest ? earliest : next
}
