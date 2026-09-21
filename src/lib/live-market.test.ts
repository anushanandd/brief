import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import seed from '../data/seed.json'
import { startMarketStream, stopMarketStream } from './api'
import { applyLiveProjection, subscribeLiveMarket } from './live-market'
import { financeSnapshotSchema, marketSnapshotsSchema, type MarketProjection } from './schema'

vi.mock('./api', () => ({
  startMarketStream: vi.fn(),
  stopMarketStream: vi.fn(),
}))
const start = vi.mocked(startMarketStream)
const stop = vi.mocked(stopMarketStream)
beforeEach(() => {
  vi.useFakeTimers()
  vi.stubGlobal('window', new EventTarget())
  start.mockReset().mockResolvedValue(undefined)
  stop.mockReset().mockResolvedValue(undefined)
})
afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

it('rejects stale revisions and timestamps and retains immutable history references', () => {
  const saved = { ...financeSnapshotSchema.parse(seed), revision: 4 }
  const projection: MarketProjection = {
    ...saved,
    netWorth: 123,
    brokeragePerformance: saved.brokeragePerformance.map(({ accountId, currentValue }) => ({
      accountId,
      currentValue: (currentValue ?? 0) + 1,
    })),
  }
  expect(applyLiveProjection(saved, { ...projection, revision: 3 })).toBe(saved)
  expect(applyLiveProjection(saved, { ...projection, updatedAt: 'old' })).toBe(saved)
  const live = applyLiveProjection(saved, projection)
  expect(live.netWorth).toBe(123)
  expect(live.transactions).toBe(saved.transactions)
  expect(live.netWorthHistory).toBe(saved.netWorthHistory)
  saved.brokeragePerformance.forEach((account, index) =>
    expect(live.brokeragePerformance[index].points).toBe(account.points),
  )
  expect(saved.netWorth).not.toBe(123)
})

it('atomically replaces every current valuation while retaining committed evidence', () => {
  const saved = { ...financeSnapshotSchema.parse(seed), revision: 4 }
  const accounts = saved.accounts.map((account, index) => ({
    ...account,
    value: account.value == null ? null : account.value + index + 10,
  }))
  const holdings = saved.holdings.map((holding, index) => ({
    ...holding,
    price: holding.price == null ? null : holding.price + index + 1,
    value: holding.value == null ? null : holding.value + index + 20,
  }))
  const projection: MarketProjection = {
    revision: 4,
    updatedAt: saved.updatedAt,
    netWorth: saved.netWorth + 100,
    netWorthIncomplete: saved.netWorthIncomplete,
    accounts,
    holdings,
    brokeragePerformance: saved.brokeragePerformance.map(({ accountId, currentValue }, index) => ({
      accountId,
      currentValue: currentValue + index + 30,
    })),
  }

  const live = applyLiveProjection(saved, projection)
  expect(live.netWorth).toBe(projection.netWorth)
  expect(live.accounts).toEqual(accounts)
  expect(live.holdings).toBe(holdings)
  expect(live.brokeragePerformance.map(({ currentValue }) => currentValue)).toEqual(
    projection.brokeragePerformance.map(({ currentValue }) => currentValue),
  )
  expect(live.transactions).toBe(saved.transactions)
  expect(live.accountBalanceHistory).toBe(saved.accountBalanceHistory)
})

it('scopes restarts and ignores old updates, failures, and post-disposal callbacks', async () => {
  const update = vi.fn(),
    error = vi.fn()
  const dispose = subscribeLiveMarket(['TEST'], 10, update, error)
  const first = start.mock.calls[0]
  window.dispatchEvent(new Event('focus'))
  expect(start).toHaveBeenCalledTimes(1)
  first[3]({ ...closedMarket(), nextTransitionAt: new Date(Date.now() + 1000).toISOString() })
  update.mockClear()
  await vi.advanceTimersByTimeAsync(2000)
  const second = start.mock.calls[1]
  expect(second[2]).not.toBe(first[2])
  expect(stop).toHaveBeenCalledWith(first[2])
  const market = closedMarket()
  first[3](market)
  first[4]('obsolete')
  expect(update).not.toHaveBeenCalled()
  expect(error).not.toHaveBeenCalled()
  second[3](market)
  expect(update).toHaveBeenCalledTimes(1)
  dispose()
  expect(stop).toHaveBeenLastCalledWith(second[2])
  second[3](market)
  second[4]('late')
  window.dispatchEvent(new Event('focus'))
  await vi.advanceTimersByTimeAsync(600000)
  expect(update).toHaveBeenCalledTimes(1)
  expect(error).not.toHaveBeenCalled()
  expect(start).toHaveBeenCalledTimes(2)
})

it('waits for the market transition when closed and retries a failed start', async () => {
  const dispose = subscribeLiveMarket(['TEST'], 10, vi.fn(), vi.fn())
  start.mock.calls[0][3]({
    ...closedMarket(),
    nextTransitionAt: new Date(Date.now() + 3600000).toISOString(),
  })
  await vi.advanceTimersByTimeAsync(3600000)
  expect(start).toHaveBeenCalledTimes(1)
  await vi.advanceTimersByTimeAsync(1000)
  expect(start).toHaveBeenCalledTimes(2)
  start.mock.calls[1][4]('temporary failure')
  await vi.advanceTimersByTimeAsync(60000)
  expect(start).toHaveBeenCalledTimes(3)
  dispose()
})

it('stops a pending startup and discards its late rejection', async () => {
  let reject!: (error: Error) => void
  start.mockImplementationOnce(
    () =>
      new Promise((_, failed) => {
        reject = failed
      }),
  )
  const error = vi.fn()
  const dispose = subscribeLiveMarket(['TEST'], 10, vi.fn(), error)
  dispose()
  expect(stop).toHaveBeenCalledWith(start.mock.calls[0][2])
  reject(new Error('late startup'))
  await vi.advanceTimersByTimeAsync(60000)
  expect(error).not.toHaveBeenCalled()
  expect(start).toHaveBeenCalledTimes(1)
})

function closedMarket() {
  return marketSnapshotsSchema.parse({
    snapshots: {},
    session: 'Market closed',
    feed: 'iex',
    delayMinutes: 0,
    asOf: null,
    nextTransitionAt: null,
    pollIntervalMs: null,
    historyFeed: 'iex',
    historyDelayMinutes: 0,
  })
}

it('keeps the native connection across focus and REST reconciliation cadence', async () => {
  const dispose = subscribeLiveMarket(['TEST'], 10000, vi.fn(), vi.fn())
  start.mock.calls[0][3]({
    ...closedMarket(),
    session: 'Regular market',
    feed: 'sip',
    pollIntervalMs: 300000,
    nextTransitionAt: new Date(Date.now() + 3600000).toISOString(),
  })
  window.dispatchEvent(new Event('focus'))
  await vi.advanceTimersByTimeAsync(600000)
  expect(start).toHaveBeenCalledTimes(1)
  expect(stop).not.toHaveBeenCalled()
  dispose()
})

it('keeps annotation-aware account income and classified ledger evidence through price ticks', () => {
  const saved = { ...financeSnapshotSchema.parse(seed), revision: 4 }
  saved.accounts[0].investmentIncomeYtd = 12
  const projection: MarketProjection = {
    ...saved,
    accounts: saved.accounts.map((account) => ({
      ...account,
      value: 100,
      investmentIncomeYtd: 999,
    })),
  }
  const live = applyLiveProjection(saved, projection)
  expect(live.accounts[0].value).toBe(100)
  expect(live.accounts[0].investmentIncomeYtd).toBe(12)
  expect(live.transactions).toBe(saved.transactions)
  expect(live.spending).toBe(saved.spending)
})
