import { afterEach, expect, it, vi } from 'vitest'

import { pageShortcutBlocked, shortcutInput } from './keyboard'

class Target extends EventTarget {
  constructor(private readonly input: boolean) {
    super()
  }
  closest() {
    return this.input ? this : null
  }
}

afterEach(() => vi.unstubAllGlobals())

it('leaves consumed and composing key events to their controls', () => {
  const consumed = new Event('keydown', { cancelable: true })
  consumed.preventDefault()
  expect(pageShortcutBlocked(consumed)).toBe(true)
  const composing = Object.assign(new Event('keydown'), { isComposing: true })
  expect(pageShortcutBlocked(composing)).toBe(true)
})

it('blocks page and native graph shortcuts for overlays or focused input controls', () => {
  vi.stubGlobal('Element', Target)
  const documentState = {
    activeElement: new Target(false),
    querySelectorAll: vi.fn((): Array<{ hasAttribute: (name: string) => boolean }> => []),
  }
  vi.stubGlobal('document', documentState)
  expect(pageShortcutBlocked()).toBe(false)
  documentState.querySelectorAll.mockReturnValue([{ hasAttribute: () => false }])
  expect(pageShortcutBlocked()).toBe(true)
  documentState.querySelectorAll.mockReturnValue([])
  documentState.activeElement = new Target(true)
  expect(shortcutInput(documentState.activeElement)).toBe(true)
  expect(pageShortcutBlocked()).toBe(true)
  expect(shortcutInput(null)).toBe(false)
})

it('allows browser and native shortcuts with a tooltip open, but still blocks another overlay', () => {
  vi.stubGlobal('Element', Target)
  const tooltip = {
    hasAttribute: (name: string): boolean => name === 'data-base-ui-tooltip-trigger',
  }
  const documentState = {
    activeElement: new Target(false),
    querySelectorAll: vi.fn(() => [tooltip]),
  }
  vi.stubGlobal('document', documentState)
  const event = { defaultPrevented: false, target: documentState.activeElement }
  expect(pageShortcutBlocked(event)).toBe(false)
  expect(pageShortcutBlocked()).toBe(false)
  documentState.querySelectorAll.mockReturnValue([tooltip, { hasAttribute: () => false }])
  expect(pageShortcutBlocked(event)).toBe(true)
  expect(pageShortcutBlocked()).toBe(true)
  documentState.querySelectorAll.mockReturnValue([tooltip])
  documentState.activeElement = new Target(true)
  expect(pageShortcutBlocked()).toBe(true)
})
