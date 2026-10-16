import { useLocation, useNavigate } from '@tanstack/react-router'
import { useCallback } from 'react'

/** Settings is addressed by the URL hash (`#settings/privacy`) so it opens over any page. */
export function settingsHash(target?: string) {
  return target ? `settings/${target}` : 'settings'
}

/** `undefined` when Settings is closed, `''` for its top, otherwise a section or row id. */
export function settingsTarget(hash: string) {
  const value = hash.replace(/^#/, '')
  if (value === 'settings') return ''
  return value.startsWith('settings/') ? value.slice('settings/'.length) : undefined
}

export function useSettingsDialog() {
  const navigate = useNavigate()
  const target = useLocation({ select: (location) => settingsTarget(location.hash) })
  const show = useCallback(
    (id?: string) =>
      void navigate({ to: '.', search: true, hash: settingsHash(id), replace: true }),
    [navigate],
  )
  const close = useCallback(
    () => void navigate({ to: '.', search: true, hash: '', replace: true }),
    [navigate],
  )
  return { open: target !== undefined, target: target ?? '', show, close }
}
