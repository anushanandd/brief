const DAY_SECONDS = 24 * 60 * 60

export const graphWindows = [
  { label: '1W', settingsLabel: '1 week', secs: 7 * DAY_SECONDS },
  { label: '1M', settingsLabel: '1 month', secs: 30 * DAY_SECONDS },
  { label: '3M', settingsLabel: '3 months', secs: 90 * DAY_SECONDS },
  { label: 'All', settingsLabel: 'All time', secs: 0 },
]

const defaultGraphWindowKey = 'brief.defaultGraphWindow'

export function parseGraphWindow(value: unknown) {
  if (value === null || value === undefined || value === '') return graphWindows[0].secs
  const seconds = Number(value)
  return graphWindows.find((window) => window.secs === seconds)?.secs ?? graphWindows[0].secs
}

export function getDefaultGraphWindow() {
  return typeof window === 'undefined'
    ? graphWindows[0].secs
    : parseGraphWindow(window.localStorage.getItem(defaultGraphWindowKey))
}

export function saveDefaultGraphWindow(seconds: number) {
  window.localStorage.setItem(defaultGraphWindowKey, String(parseGraphWindow(seconds)))
}
