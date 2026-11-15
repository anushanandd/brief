import { describe, expect, it, vi } from 'vitest'

import {
  adjacentGraphWindow,
  parseGraphWindow,
  shiftedGraphEnd,
} from './graph-preferences'

const day = 24 * 60 * 60

it('scrolls a range back within the history and ends today again at the latest day', () => {
  const today = '2026-09-30'
  const first = '2026-01-01'
  const month = 30 * day
  // A period back from today, then another, then ahead again.
  expect(shiftedGraphEnd(undefined, -30, month, today, first)).toBe('2026-08-31')
  expect(shiftedGraphEnd('2026-08-31', -30, month, today, first)).toBe('2026-08-01')
  expect(shiftedGraphEnd('2026-08-31', 30, month, today, first)).toBeUndefined()
  // Never before a whole period after the first day.
  expect(shiftedGraphEnd('2026-02-05', -30, month, today, first)).toBe('2026-01-31')
  // A history shorter than the range, a day, or all time doesn't scroll.
  expect(shiftedGraphEnd(undefined, -30, month, today, '2026-09-15')).toBeUndefined()
  expect(shiftedGraphEnd(undefined, -1, day, today, first)).toBeUndefined()
  expect(shiftedGraphEnd(undefined, -30, 0, today, first)).toBeUndefined()
  expect(shiftedGraphEnd(undefined, -30, month, today, undefined)).toBeUndefined()
})

it('counts a page as scrolling for a moment after its end day moves', async () => {
  const { renderToStaticMarkup } = await import('react-dom/server')
  const { createElement } = await import('react')
  const { setGraphEnd, useGraphScrolling, resetSessionGraphWindow } =
    await import('./graph-preferences')
  vi.useFakeTimers({ toFake: ['Date'] })
  const scrolling = () => {
    let seen = false
    renderToStaticMarkup(
      createElement(() => {
        seen = useGraphScrolling()
        return null
      }),
    )
    return seen
  }
  setGraphEnd('2026-08-31')
  expect(scrolling()).toBe(true)
  vi.advanceTimersByTime(1_500)
  expect(scrolling()).toBe(false)
  vi.useRealTimers()
  resetSessionGraphWindow()
})

it('ends today again when the range changes to one that does not scroll', async () => {
  const { renderToStaticMarkup } = await import('react-dom/server')
  const { createElement } = await import('react')
  const { setGraphEnd, useGraphWindow, resetSessionGraphWindow, sessionGraphEnd } =
    await import('./graph-preferences')
  resetSessionGraphWindow()
  let setWindow: ReturnType<typeof useGraphWindow>[1] | undefined
  renderToStaticMarkup(
    createElement(() => {
      ;[, setWindow] = useGraphWindow()
      return null
    }),
  )
  setWindow!(30 * day)
  setGraphEnd('2026-08-31')
  setWindow!(90 * day)
  expect(sessionGraphEnd()).toBe('2026-08-31')
  setWindow!(0)
  expect(sessionGraphEnd()).toBeUndefined()
  resetSessionGraphWindow()
})

describe('graph preferences', () => {
  it('validates saved ranges and wraps keyboard selection', () => {
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
