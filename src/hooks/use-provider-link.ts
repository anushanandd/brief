import { useEffect, useRef, useState } from 'react'
import { toast } from 'sonner'

import { beginProviderLink, cancelProviderLink, pollProviderLink } from '../lib/api'
import { errorMessage } from '../lib/errors'
import type { ProviderLinkSession } from '../lib/schema'

export type LinkProvider = ProviderLinkSession['provider']
export type LinkPhase = 'browser' | 'checking' | 'refreshing'

export const linkProviderNames: Record<LinkProvider, string> = {
  plaid: 'Plaid',
  'plaid-investments': 'Plaid Investments',
  snaptrade: 'SnapTrade',
}

const wait = (duration: number) => new Promise((resolve) => window.setTimeout(resolve, duration))
const pollInterval = 2_000
const pollAttempts = 150

/**
 * A provider link finished in the browser: begin it, poll until the provider reports it
 * connected, then refresh. One link runs at a time; cancelling, unmounting, or starting another
 * leaves an earlier attempt's late answers unread.
 */
export function useProviderLink({
  refresh,
  onConnected,
}: {
  refresh: () => Promise<unknown>
  /** Runs once the refresh after a connection finishes, if the attempt is still current. */
  onConnected: (provider: LinkProvider) => void
}) {
  const [linking, setLinking] = useState<LinkProvider | null>(null)
  const [phase, setPhase] = useState<LinkPhase>('browser')
  const browserCompleted = useRef(false)
  const attempt = useRef(0)
  const session = useRef<ProviderLinkSession | null>(null)
  const active = useRef<LinkProvider | null>(null)

  /** Ends the current attempt, so its late answers are ignored, and returns its session. */
  const release = () => {
    attempt.current += 1
    active.current = null
    const current = session.current
    session.current = null
    return current
  }

  useEffect(
    () => () => {
      const current = release()
      if (current) void cancelProviderLink(current).catch(() => undefined)
    },
    [],
  )

  const cancel = async () => {
    const current = release()
    setLinking(null)
    if (current)
      await cancelProviderLink(current).catch(() =>
        toast.error('Could not cancel the connection check'),
      )
  }

  const connect = async (provider: LinkProvider, itemId?: string) => {
    if (active.current) {
      toast.info(`Finish the ${linkProviderNames[active.current]} connection first`)
      return
    }
    const attemptId = attempt.current + 1
    attempt.current = attemptId
    const stale = () => attempt.current !== attemptId
    active.current = provider
    setLinking(provider)
    setPhase('browser')
    browserCompleted.current = false
    try {
      const started = await beginProviderLink(provider, itemId)
      if (stale()) {
        await cancelProviderLink(started)
        return
      }
      session.current = started
      toast.info('Finish connecting in your browser')
      for (let poll = 0; poll < pollAttempts; poll += 1) {
        await wait(pollInterval)
        if (stale()) return
        const result = await pollProviderLink(started, browserCompleted.current)
        if (stale()) return
        if (result.status === 'connected') {
          setPhase('refreshing')
          await refresh()
          if (stale()) return
          onConnected(provider)
          return
        }
      }
      throw new Error('The connection window expired. Try again when you are ready.')
    } catch (error) {
      if (!stale()) toast.error(errorMessage(error))
    } finally {
      if (!stale()) {
        const current = release()
        if (current) void cancelProviderLink(current).catch(() => undefined)
        setLinking(null)
      }
    }
  }

  /** The user says the browser step is done, so the next check can finish a SnapTrade link. */
  const browserDone = () => {
    browserCompleted.current = true
    setPhase('checking')
  }

  return { linking, phase, connect, cancel, browserDone }
}
