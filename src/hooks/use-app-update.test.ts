import { toast } from 'sonner'
import { beforeEach, expect, it, vi } from 'vitest'

import { checkAppUpdate, installAppUpdate } from '../lib/api'
import { checkForUpdates, installUpdate, releaseUrl } from './use-app-update'

vi.mock('sonner', () => ({
  toast: Object.assign(vi.fn(), { success: vi.fn(), error: vi.fn(), loading: vi.fn() }),
}))
vi.mock('../lib/api', () => ({
  isTauri: () => true,
  checkAppUpdate: vi.fn(),
  installAppUpdate: vi.fn(),
  openExternalUrl: vi.fn(),
}))

beforeEach(() => vi.clearAllMocks())

it('offers a newer release with its first line of notes', async () => {
  vi.mocked(checkAppUpdate).mockResolvedValueOnce({ version: '0.2.0', notes: '\nFaster pages' })
  await checkForUpdates(false)
  expect(toast).toHaveBeenCalledWith(
    'Brief 0.2.0 is available',
    expect.objectContaining({
      description: 'Faster pages',
      action: expect.objectContaining({ label: 'Install and Restart' }),
    }),
  )
})

it('says why an install failed', async () => {
  vi.mocked(installAppUpdate).mockRejectedValueOnce(new Error('signature mismatch'))
  await installUpdate('0.2.0')
  expect(toast.error).toHaveBeenCalledWith('Could not install the update', {
    id: 'app-update',
    description: 'signature mismatch',
  })
})

it('says so when asked and up to date, and stays quiet when scheduled', async () => {
  vi.mocked(checkAppUpdate).mockResolvedValue(null)
  await checkForUpdates(false)
  expect(toast.success).not.toHaveBeenCalled()
  await checkForUpdates(true)
  expect(toast.success).toHaveBeenCalledWith('Brief is up to date')
})

it('reports a failed check only when asked', async () => {
  vi.mocked(checkAppUpdate).mockRejectedValue(new Error('offline'))
  await checkForUpdates(false)
  expect(toast.error).not.toHaveBeenCalled()
  await checkForUpdates(true)
  expect(toast.error).toHaveBeenCalledWith('Could not check for updates', {
    description: 'offline',
  })
})

it('links a version to its release page', () => {
  expect(releaseUrl('0.2.0')).toBe('https://github.com/anushanandd/brief/releases/tag/v0.2.0')
})
