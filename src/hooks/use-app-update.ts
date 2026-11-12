import { useEffect } from 'react'
import { toast } from 'sonner'

import { checkAppUpdate, installAppUpdate, isTauri, openExternalUrl } from '../lib/api'
import type { AppUpdate } from '../lib/bindings/AppUpdate'
import { errorMessage } from '../lib/errors'

const DAY = 24 * 60 * 60 * 1000
const TOAST_ID = 'app-update'

/** The release page for a version, which lists what changed. */
export function releaseUrl(version: string) {
  return `https://github.com/anushanandd/brief/releases/tag/v${version}`
}

/** Installs and relaunches; the toast shows progress and, if it fails, why. */
export async function installUpdate(version: string) {
  toast.loading(`Installing Brief ${version}…`, { id: TOAST_ID })
  try {
    await installAppUpdate()
  } catch (error) {
    toast.error('Could not install the update', { id: TOAST_ID, description: errorMessage(error) })
  }
}

function offer(update: AppUpdate) {
  toast(`Brief ${update.version} is available`, {
    id: TOAST_ID,
    duration: Infinity,
    description: update.notes?.split('\n').find((line) => line.trim()) ?? undefined,
    action: { label: 'Install and Restart', onClick: () => void installUpdate(update.version) },
    cancel: {
      label: 'What’s New',
      onClick: () => void openExternalUrl(releaseUrl(update.version)).catch(() => undefined),
    },
  })
}

/**
 * Looks for a newer release and offers it. `asked` is true when the person chose Check for
 * Updates, so being up to date or offline is worth saying; scheduled checks stay quiet then.
 */
export async function checkForUpdates(asked: boolean) {
  if (!isTauri()) return
  try {
    const update = await checkAppUpdate()
    if (update) offer(update)
    else if (asked) toast.success('Brief is up to date')
  } catch (error) {
    if (asked) toast.error('Could not check for updates', { description: errorMessage(error) })
  }
}

/** Checks at launch and once a day after. */
export function useAppUpdateChecks() {
  useEffect(() => {
    void checkForUpdates(false)
    const timer = window.setInterval(() => void checkForUpdates(false), DAY)
    return () => window.clearInterval(timer)
  }, [])
}
