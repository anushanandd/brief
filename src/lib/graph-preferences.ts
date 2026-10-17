import { useSyncExternalStore, type Dispatch, type SetStateAction } from 'react'

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
  return graphWindows[(Math.max(0, index) + direction + graphWindows.length) % graphWindows.length]
    .secs
}

export function parseGraphWindow(value: unknown) {
  if (value === null || value === undefined || value === '') return DEFAULT_GRAPH_WINDOW
  const seconds = Number(value)
  return graphWindows.find((window) => window.secs === seconds)?.secs ?? DEFAULT_GRAPH_WINDOW
}

const defaultWindow = preference('brief.defaultGraphWindow', parseGraphWindow)

export const getDefaultGraphWindow = defaultWindow.get
export const saveDefaultGraphWindow = (seconds: number) =>
  defaultWindow.save(parseGraphWindow(seconds))

// One range for this session, shared by every page with a D/W/M/Q/Y/A chart, so moving between
// pages keeps the period in view. It starts from the saved default and is not persisted.
let sessionWindow: number | undefined
const listeners = new Set<() => void>()
const currentWindow = () => sessionWindow ?? getDefaultGraphWindow()
const setSessionWindow: Dispatch<SetStateAction<number>> = (next) => {
  const value = parseGraphWindow(typeof next === 'function' ? next(currentWindow()) : next)
  if (value === currentWindow()) return
  sessionWindow = value
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
}
