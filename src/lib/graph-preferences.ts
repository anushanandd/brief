const DAY_SECONDS = 24 * 60 * 60
const DEFAULT_GRAPH_WINDOW = 7 * DAY_SECONDS

export const graphWindows = [
  { label: 'W', settingsLabel: 'Week', secs: 7 * DAY_SECONDS },
  { label: 'M', settingsLabel: 'Month', secs: 30 * DAY_SECONDS },
  { label: 'Q', settingsLabel: 'Quarter', secs: 90 * DAY_SECONDS },
  { label: 'Y', settingsLabel: 'Year', secs: 365 * DAY_SECONDS },
  { label: 'A', settingsLabel: 'All time', secs: 0 },
]

export function graphWindowForKey(key: string) {
  return graphWindows[['w', 'm', 'q', 'y', 'a'].indexOf(key.toLocaleLowerCase())]?.secs
}

export function adjacentGraphWindow(current: number, direction: -1 | 1) {
  const index = graphWindows.findIndex(({ secs }) => secs === current)
  return graphWindows[(Math.max(0, index) + direction + graphWindows.length) % graphWindows.length]
    .secs
}

const defaultGraphWindowKey = 'brief.defaultGraphWindow'
const chartAccountPreferencesKey = 'brief.chartAccountPreferences'

export type ChartAccountPreference = { accountId: string; visible: boolean }

export function parseGraphWindow(value: unknown) {
  if (value === null || value === undefined || value === '') return DEFAULT_GRAPH_WINDOW
  const seconds = Number(value)
  return graphWindows.find((window) => window.secs === seconds)?.secs ?? DEFAULT_GRAPH_WINDOW
}

export function getDefaultGraphWindow() {
  return typeof window === 'undefined'
    ? DEFAULT_GRAPH_WINDOW
    : parseGraphWindow(window.localStorage.getItem(defaultGraphWindowKey))
}

export function saveDefaultGraphWindow(seconds: number) {
  window.localStorage.setItem(defaultGraphWindowKey, String(parseGraphWindow(seconds)))
}

export function parseChartAccountPreferences(value: unknown): ChartAccountPreference[] {
  if (typeof value !== 'string') return []
  try {
    const parsed: unknown = JSON.parse(value)
    if (!Array.isArray(parsed)) return []
    const seen = new Set<string>()
    return parsed.flatMap((item) => {
      if (
        typeof item !== 'object' ||
        item === null ||
        !('accountId' in item) ||
        typeof item.accountId !== 'string' ||
        !item.accountId ||
        seen.has(item.accountId) ||
        !('visible' in item) ||
        typeof item.visible !== 'boolean'
      )
        return []
      seen.add(item.accountId)
      return [{ accountId: item.accountId, visible: item.visible }]
    })
  } catch {
    return []
  }
}

export function getChartAccountPreferences() {
  return typeof window === 'undefined'
    ? []
    : parseChartAccountPreferences(window.localStorage.getItem(chartAccountPreferencesKey))
}

export function saveChartAccountPreferences(preferences: ChartAccountPreference[]) {
  window.localStorage.setItem(chartAccountPreferencesKey, JSON.stringify(preferences))
}

export function reconcileChartAccountPreferences(
  accountIds: string[],
  preferences: ChartAccountPreference[],
) {
  const available = new Set(accountIds)
  const saved = preferences.filter(({ accountId }) => available.has(accountId))
  const savedIds = new Set(saved.map(({ accountId }) => accountId))
  return [
    ...saved,
    ...accountIds
      .filter((accountId) => !savedIds.has(accountId))
      .map((accountId) => ({ accountId, visible: true })),
  ]
}
