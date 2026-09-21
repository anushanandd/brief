import { useSyncExternalStore } from 'react'

const marketUpdateIntervalKey = 'brief.marketUpdateIntervalSeconds'
const DEFAULT_MARKET_UPDATE_INTERVAL_SECONDS = 10

export const marketUpdateIntervals = [
  { label: '5s', settingsLabel: '5 seconds', seconds: 5 },
  { label: '10s', settingsLabel: '10 seconds', seconds: 10 },
  { label: '30s', settingsLabel: '30 seconds', seconds: 30 },
  { label: '1m', settingsLabel: '1 minute', seconds: 60 },
]

export function parseMarketUpdateInterval(value: unknown) {
  const seconds = Number(value)
  return (
    marketUpdateIntervals.find((option) => option.seconds === seconds)?.seconds ??
    DEFAULT_MARKET_UPDATE_INTERVAL_SECONDS
  )
}

export function getMarketUpdateInterval() {
  return typeof window === 'undefined'
    ? DEFAULT_MARKET_UPDATE_INTERVAL_SECONDS
    : parseMarketUpdateInterval(window.localStorage.getItem(marketUpdateIntervalKey))
}

export function saveMarketUpdateInterval(seconds: number) {
  window.localStorage.setItem(marketUpdateIntervalKey, String(parseMarketUpdateInterval(seconds)))
  window.dispatchEvent(new Event(marketUpdateIntervalKey))
}

function subscribe(listener: () => void) {
  window.addEventListener(marketUpdateIntervalKey, listener)
  window.addEventListener('storage', listener)
  return () => {
    window.removeEventListener(marketUpdateIntervalKey, listener)
    window.removeEventListener('storage', listener)
  }
}

export function useMarketUpdateInterval() {
  return useSyncExternalStore(
    subscribe,
    getMarketUpdateInterval,
    () => DEFAULT_MARKET_UPDATE_INTERVAL_SECONDS,
  )
}
