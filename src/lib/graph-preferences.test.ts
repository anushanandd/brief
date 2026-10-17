import { describe, expect, it } from 'vitest'

import { adjacentGraphWindow, graphWindowForKey, parseGraphWindow } from './graph-preferences'

describe('graph preferences', () => {
  it('validates saved ranges and wraps keyboard selection', () => {
    const day = 24 * 60 * 60
    expect(parseGraphWindow(String(day))).toBe(day)
    expect(parseGraphWindow(String(2 * day))).toBe(7 * day)
    expect(parseGraphWindow(String(30 * day))).toBe(30 * day)
    expect(parseGraphWindow(String(90 * day))).toBe(90 * day)
    expect(parseGraphWindow(String(365 * day))).toBe(365 * day)
    expect(parseGraphWindow('0')).toBe(0)
    expect(parseGraphWindow(null)).toBe(7 * day)
    expect(adjacentGraphWindow(7 * day, -1)).toBe(day)
    expect(adjacentGraphWindow(day, -1)).toBe(0)
    expect(adjacentGraphWindow(7 * day, 1)).toBe(30 * day)
    expect(adjacentGraphWindow(30 * day, 1)).toBe(90 * day)
    expect(adjacentGraphWindow(90 * day, 1)).toBe(365 * day)
    expect(adjacentGraphWindow(365 * day, 1)).toBe(0)
    expect(adjacentGraphWindow(0, 1)).toBe(day)
    expect(graphWindowForKey('d')).toBe(day)
    expect(graphWindowForKey('w')).toBe(7 * day)
    expect(graphWindowForKey('M')).toBe(30 * day)
    expect(graphWindowForKey('q')).toBe(90 * day)
    expect(graphWindowForKey('y')).toBe(365 * day)
    expect(graphWindowForKey('a')).toBe(0)
    expect(graphWindowForKey('x')).toBeUndefined()
  })
})

it('shares one session range between pages and ignores values that are not a range', async () => {
  const { renderToStaticMarkup } = await import('react-dom/server')
  const { createElement } = await import('react')
  const { useGraphWindow, resetSessionGraphWindow } = await import('./graph-preferences')
  resetSessionGraphWindow()
  let setter: ReturnType<typeof useGraphWindow>[1] | undefined
  const Probe = () => {
    const [, setWindow] = useGraphWindow()
    setter = setWindow
    return null
  }
  renderToStaticMarkup(createElement(Probe))
  const set = setter
  if (!set) throw new Error('The probe did not render')
  const read = () => {
    let value = -1
    const Reader = () => {
      ;[value] = useGraphWindow()
      return null
    }
    renderToStaticMarkup(createElement(Reader))
    return value
  }
  set(365 * 24 * 60 * 60)
  set((current) => current)
  // Server rendering reads the default; the store itself is checked through a second setter.
  let seen = 0
  set((current) => {
    seen = current
    return current
  })
  expect(seen).toBe(365 * 24 * 60 * 60)
  set(123)
  set((current) => {
    seen = current
    return current
  })
  expect(seen).toBe(7 * 24 * 60 * 60)
  expect(read()).toBe(7 * 24 * 60 * 60)
  resetSessionGraphWindow()
})
