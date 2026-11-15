// @vitest-environment jsdom
import { act, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import {
  resetSessionGraphWindow,
  sessionGraphEnd,
  type GraphScroll,
} from '../lib/graph-preferences'
import { useChartPan } from './charts'

const day = 86_400
const week = 7 * day

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'] })
  vi.setSystemTime(new Date('2026-09-30T18:00:00Z'))
  resetSessionGraphWindow()
})
afterEach(() => {
  vi.useRealTimers()
  resetSessionGraphWindow()
})

it('moves the end day as each day passes, through redraws mid-scroll, and settles on a day', async () => {
  let pan = 0
  let setRedraws: ((redraws: number) => void) | undefined
  function Chart() {
    const [element, setElement] = useState<HTMLDivElement | null>(null)
    ;[, setRedraws] = useState(0)
    // A fresh scroll each draw, as a page gives when new figures or prices arrive.
    const scroll: GraphScroll = { today: '2026-09-30', first: '2026-01-01' }
    pan = useChartPan(element, week, scroll)
    return <div ref={setElement} />
  }
  const host = document.createElement('div')
  const root = createRoot(host)
  await act(async () => root.render(<Chart />))
  const chart = host.querySelector('div')!
  // 700 pixels show the week, so each 100 scrolled is a day.
  Object.defineProperty(chart, 'clientWidth', { value: 700 })
  const wheel = (deltaY: number) =>
    act(async () => {
      chart.dispatchEvent(new WheelEvent('wheel', { deltaY, bubbles: true, cancelable: true }))
    })

  await wheel(150)
  // A day and a half back from 6 pm: yesterday's noon, and a quarter day drawn before it.
  expect(sessionGraphEnd()).toBe('2026-09-29')
  expect(pan).toBe(-day / 4)
  await act(async () => setRedraws!(1))
  // Three days back, nothing lost to the redraw: the 28th, three quarters of a day before it.
  await wheel(150)
  expect(sessionGraphEnd()).toBe('2026-09-28')
  expect(pan).toBe((-day * 3) / 4)

  // Paused, it settles on the nearest day.
  await act(async () => vi.advanceTimersByTime(200))
  expect(sessionGraphEnd()).toBe('2026-09-27')
  expect(pan).toBe(0)
  await act(async () => root.unmount())
})
