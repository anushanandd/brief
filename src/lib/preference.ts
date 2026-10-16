import { useSyncExternalStore } from 'react'

/**
 * One value kept in this browser's storage: a per-viewer convenience such as a theme or a
 * remembered layout, never a financial result. Reads tolerate missing or blocked storage and
 * return the parsed default; a save announces the change to every subscriber in this window
 * under the key's name, and other windows hear it through the storage event.
 */
export type Preference<T> = {
  readonly key: string
  /** The saved value, or the default when nothing valid is saved. */
  get: () => T
  save: (value: T) => void
  /** The value, re-rendering when it changes. Server rendering sees the default. */
  use: () => T
}

export function preference<T>(
  key: string,
  parse: (raw: string | null) => T,
  serialize: (value: T) => string = String,
): Preference<T> {
  // Parsed once per raw string, so React sees one reference while nothing changed.
  let cached: { raw: string | null; value: T } | undefined
  const read = (): string | null => {
    try {
      return typeof window === 'undefined' ? null : (window.localStorage?.getItem(key) ?? null)
    } catch {
      return null
    }
  }
  const get = () => {
    const raw = read()
    if (!cached || cached.raw !== raw) cached = { raw, value: parse(raw) }
    return cached.value
  }
  const save = (value: T) => {
    try {
      window.localStorage?.setItem(key, serialize(value))
    } catch {
      // Storage is blocked; the choice holds for this session's subscribers only.
    }
    window.dispatchEvent(new Event(key))
  }
  const subscribe = (notify: () => void) => {
    window.addEventListener(key, notify)
    window.addEventListener('storage', notify)
    return () => {
      window.removeEventListener(key, notify)
      window.removeEventListener('storage', notify)
    }
  }
  const fallback = () => parse(null)
  return { key, get, save, use: () => useSyncExternalStore(subscribe, get, fallback) }
}
