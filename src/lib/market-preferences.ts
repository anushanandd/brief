import { preference } from './preference'

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

const interval = preference('brief.marketUpdateIntervalSeconds', parseMarketUpdateInterval)

export const getMarketUpdateInterval = interval.get
export const saveMarketUpdateInterval = (seconds: number) =>
  interval.save(parseMarketUpdateInterval(seconds))
export const useMarketUpdateInterval = interval.use
