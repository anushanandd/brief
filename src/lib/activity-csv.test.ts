import { invoke } from '@tauri-apps/api/core'
import { expect, it, vi } from 'vitest'

import type { ActivityItem } from './activity'
import { activityCsv, activityCsvFilename, downloadActivityCsv } from './activity-csv'

vi.mock('./api', () => ({ isTauri: () => true }))
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }))

it('passes CSV to the native save panel and preserves cancellation and failure', async () => {
  vi.mocked(invoke).mockResolvedValueOnce(false)
  expect(await downloadActivityCsv([], 'cash', 'Example checking')).toBe(false)
  expect(invoke).toHaveBeenCalledWith('export_finance_csv', {
    contents: activityCsv([]),
    filename: 'Example checking-activity.csv',
  })
  vi.mocked(invoke).mockRejectedValueOnce(new Error('Could not save'))
  await expect(downloadActivityCsv([], 'cash', 'Example checking')).rejects.toThrow(
    'Could not save',
  )
})

it('uses safe account-based filenames', () => {
  expect(activityCsvFilename(' Example Checking ')).toBe('Example Checking-activity.csv')
  expect(activityCsvFilename('../Example/Checking')).toBe('-Example-Checking-activity.csv')
  expect(activityCsvFilename('')).toBe('Account-activity.csv')
})

it('exports only the selected account, retaining safe text and unchanged amounts', async () => {
  const activities: ActivityItem[] = [
    {
      id: 'spending:a',
      accountId: 'cash',
      date: '2026-09-01',
      kind: 'transaction',
      title: '=SUM(1,2)',
      category: 'Other',
      location: { city: 'San Francisco', region: 'CA' },
      paymentChannel: 'in store',
      detail: 'Example "account"\nline',
      amount: -12.5,
      pending: true,
    },
    {
      id: 'trade:b',
      accountId: 'brokerage',
      date: '2026-09-02',
      kind: 'trade',
      title: 'Sold TEST',
      category: 'Trade',
      detail: 'Example brokerage',
      amount: 24,
    },
  ]
  const csv = activityCsv(activities)
  expect(csv).toContain('"\'=SUM(1,2)"')
  expect(csv).toContain('"Example ""account""\nline"')
  expect(csv).toContain('"San Francisco, CA"')
  expect(csv).toContain('"In store"')
  expect(csv).toContain('"Brokerage"')
  expect(csv).toContain('"-12.5","true"')
  expect(csv).toContain('"trade:b","brokerage"')
  expect(csv).toContain('"24",""\r\n')
  vi.mocked(invoke).mockResolvedValue(true)
  await downloadActivityCsv(activities, 'cash', 'Checking')
  expect(invoke).toHaveBeenLastCalledWith('export_finance_csv', {
    contents: activityCsv([activities[0]]),
    filename: 'Checking-activity.csv',
  })
  await downloadActivityCsv(activities, 'brokerage', 'Investments')
  expect(invoke).toHaveBeenLastCalledWith('export_finance_csv', {
    contents: activityCsv([activities[1]]),
    filename: 'Investments-activity.csv',
  })
  await downloadActivityCsv(activities, 'missing', 'Empty')
  expect(invoke).toHaveBeenLastCalledWith('export_finance_csv', {
    contents: activityCsv([]),
    filename: 'Empty-activity.csv',
  })
  await downloadActivityCsv(activities, 'all', 'All accounts')
  expect(invoke).toHaveBeenLastCalledWith('export_finance_csv', {
    contents: csv,
    filename: 'All accounts-activity.csv',
  })
  expect(activityCsv([])).toBe(
    '"ID","Account ID","Date","Type","Description","Category","Location","Method","Detail","Amount","Pending"\r\n',
  )
})
