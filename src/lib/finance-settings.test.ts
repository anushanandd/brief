// @vitest-environment jsdom

import { invoke } from '@tauri-apps/api/core'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { financeSettings, importLegacyFinanceSettings } from './finance-settings'

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))
vi.mock('./api', () => ({ isTauri: () => true, getFinanceSnapshot: vi.fn() }))

beforeEach(() => {
  const values = new Map<string, string>()
  const storage = {
    getItem: (key: string) => values.get(key) ?? null,
    removeItem: (key: string) => values.delete(key),
    setItem: (key: string, value: string) => values.set(key, value),
  }
  Object.defineProperty(window, 'localStorage', { configurable: true, value: storage })
})
afterEach(() => vi.resetAllMocks())

it('imports legacy renderer settings once and removes them only after the native save', async () => {
  localStorage.setItem('brief:spending-account-id', 'plaid:card')
  localStorage.setItem('brief:hidden-platinum-benefits', 'resy,oura')
  localStorage.setItem('brief.accountDisplayNames', '{"plaid:card":" Card "}')
  localStorage.setItem('brief.accountStartDates', '{"plaid:card":"2026-01-01","bad":"x"}')
  vi.mocked(invoke).mockResolvedValue({})

  expect(await importLegacyFinanceSettings()).toBeUndefined()
  expect(invoke).toHaveBeenCalledWith('save_finance_settings', {
    importing: true,
    settings: {
      spendingAccountId: 'plaid:card',
      hiddenBenefitIds: ['resy', 'oura'],
      accountNames: { 'plaid:card': 'Card' },
      accountStartDates: { 'plaid:card': '2026-01-01' },
      accountClosedDates: {},
      manualAccounts: [],
      plaidLiabilities: false,
      plaidInvestmentTransactions: false,
      plaidRecurring: false,
      backgroundRefresh: false,
      notifications: false,
      calendarSync: false,
      people: [],
      tripCompanions: [],
      research: false,
    },
  })
  expect(localStorage.getItem('brief:spending-account-id')).toBeNull()
  expect(await importLegacyFinanceSettings()).toBeUndefined()
  expect(invoke).toHaveBeenCalledTimes(1)
})

it('retains legacy settings when the native store rejects them', async () => {
  localStorage.setItem('brief:spending-account-id', 'plaid:card')
  vi.mocked(invoke).mockRejectedValue(new Error('read-only recovery'))
  expect(await importLegacyFinanceSettings()).toMatch(/originals kept/)
  expect(localStorage.getItem('brief:spending-account-id')).toBe('plaid:card')
})

it('uses empty defaults when a snapshot has no saved settings', () => {
  expect(financeSettings(undefined)).toEqual({
    accountNames: {},
    accountStartDates: {},
    accountClosedDates: {},
    manualAccounts: [],
    hiddenBenefitIds: [],
    plaidLiabilities: false,
    plaidInvestmentTransactions: false,
    plaidRecurring: false,
    backgroundRefresh: false,
    notifications: false,
    calendarSync: false,
    people: [],
    tripCompanions: [],
    research: false,
  })
})
