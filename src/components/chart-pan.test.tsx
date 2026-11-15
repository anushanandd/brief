// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, memo, Profiler, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { insights } from '../data/fixtures/insights'
import { seedSnapshot } from '../data/fixtures/records'
import { useAsOf } from '../hooks/use-as-of'
import {
  resetSessionGraphWindow,
  sessionGraphEnd,
  useGraphEnd,
  type GraphScroll,
} from '../lib/graph-preferences'
import { chartDayTime } from '../lib/live-chart'
import { useChartPan } from './charts'

vi.mock('../lib/api', async (original) => ({
  ...(await original<typeof import('../lib/api')>()),
  getPeriodAsOf: vi.fn(async (end: string) => ({
    end,
    chartChanges: [],
    periods: [],
    flows: [],
    changes: [],
    recap: [],
  })),
}))

const day = 86_400
const week = 7 * day
const month = 30 * day
const now = Date.parse('2026-09-30T18:00:00Z') / 1_000
const frame = () => act(async () => vi.advanceTimersToNextFrame())

beforeEach(() => {
  vi.useFakeTimers({
    toFake: [
      'Date',
      'setTimeout',
      'clearTimeout',
      'setInterval',
      'clearInterval',
      'requestAnimationFrame',
      'cancelAnimationFrame',
    ],
  })
  vi.setSystemTime(now * 1_000)
  resetSessionGraphWindow()
})
afterEach(() => {
  vi.useRealTimers()
  resetSessionGraphWindow()
  document.body.replaceChildren()
})

function mount(node: React.ReactNode) {
  const host = document.body.appendChild(document.createElement('div'))
  const root = createRoot(host)
  return { host, root, render: () => act(async () => root.render(node)) }
}

const wheelOn = (element: Element, deltaY: number, cancelable = true) =>
  element.dispatchEvent(new WheelEvent('wheel', { deltaY, bubbles: true, cancelable }))

it('draws each frame where the scroll is, follows the last day it reaches, and eases onto a day', async () => {
  let pan: number | undefined
  let setRedraws: ((redraws: number) => void) | undefined
  function Chart() {
    const [element, setElement] = useState<HTMLDivElement | null>(null)
    ;[, setRedraws] = useState(0)
    // A fresh scroll each draw, as a page gives when new figures or prices arrive.
    const scroll: GraphScroll = { today: '2026-09-30', first: '2026-01-01' }
    pan = useChartPan(element, week, scroll)
    return <div ref={setElement} />
  }
  const { host, root, render } = mount(<Chart />)
  await render()
  const chart = host.querySelector('div')!
  // 700 pixels show the week, so each 100 scrolled is a day.
  Object.defineProperty(chart, 'clientWidth', { value: 700 })

  // Two events in one frame move the chart once, by both.
  await act(async () => {
    wheelOn(chart, 75)
    wheelOn(chart, 75)
  })
  expect(pan).toBeUndefined()
  await frame()
  // A day and a half back from 6 pm: the 29th at 6 am, so the 28th is the last day drawn.
  expect(pan).toBe(now - 1.5 * day)
  expect(sessionGraphEnd()).toBe('2026-09-28')
  await act(async () => setRedraws!(1))
  // Nothing lost to the redraw.
  await act(async () => wheelOn(chart, 150))
  await frame()
  expect(pan).toBe(now - 3 * day)

  // Paused, it eases onto the nearest day instead of jumping there.
  await act(async () => vi.advanceTimersByTime(160 + 16 * 3))
  expect(pan).toBeLessThan(now - 3 * day)
  expect(pan).toBeGreaterThan(chartDayTime('2026-09-27'))
  await act(async () => vi.advanceTimersByTime(250))
  expect(pan).toBeUndefined()
  expect(sessionGraphEnd()).toBe('2026-09-27')

  // Momentum the page started scrolling with isn't the chart's to take.
  await act(async () => wheelOn(chart, 300, false))
  await frame()
  expect(pan).toBeUndefined()
  await act(async () => root.unmount())
})

it('scrolls a month back in a handful of page commits, not several per day crossed', async () => {
  const data = seedSnapshot()
  data.insights = insights({ today: '2026-09-30' })
  const scroll: GraphScroll = { today: '2026-09-30', first: '2026-01-01' }
  let pageRendered = false
  let pageCommits = 0
  let commits = 0
  const Chart = memo(function Chart() {
    const [element, setElement] = useState<HTMLDivElement | null>(null)
    useChartPan(element, month, scroll)
    return <div className="chart" ref={setElement} />
  })
  function Page() {
    pageRendered = true
    useGraphEnd()
    useAsOf(data)
    return <Chart />
  }
  const counted = () => {
    commits += 1
    if (pageRendered) pageCommits += 1
    pageRendered = false
  }
  const { host, root, render } = mount(
    <QueryClientProvider client={new QueryClient()}>
      <Profiler id="page" onRender={counted}>
        <Page />
      </Profiler>
    </QueryClientProvider>,
  )
  await render()
  const chart = host.querySelector('.chart')!
  // 700 pixels show the month, so 700 scrolled is 30 days.
  Object.defineProperty(chart, 'clientWidth', { value: 700 })
  pageCommits = 0
  commits = 0
  for (let index = 0; index < 70; index += 1) {
    await act(async () => wheelOn(chart, 10))
    await frame()
  }
  await act(async () => vi.advanceTimersByTime(1_000))
  expect(sessionGraphEnd()).toBe('2026-08-31')
  console.info(`30 days on M: ${pageCommits} page commits, ${commits} commits`)
  // About a second of scrolling publishes the end day ten times or so, each a commit or two.
  expect(pageCommits).toBeLessThanOrEqual(25)
  await act(async () => root.unmount())
})
