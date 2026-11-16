// @vitest-environment jsdom
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, memo, Profiler, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from 'vitest'

import { insights } from '../data/fixtures/insights'
import { seedSnapshot } from '../data/fixtures/records'
import { useAsOf } from '../hooks/use-as-of'
import { addDays } from '../lib/dates'
import {
  resetSessionGraphWindow,
  sessionGraphEnd,
  useGraphEnd,
  type GraphScroll,
} from '../lib/graph-preferences'
import { chartDayTime } from '../lib/live-chart'
import { type ChartTimeline, PerformanceChart, useChartPan } from './charts'

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

it('draws the chart where the scroll is, follows the last day it reaches, and eases onto a day', async () => {
  // Each view the chart is asked to draw: its end while moving, or undefined back at its own.
  const views: Array<number | undefined> = []
  const handle = {
    current: { view: (end?: number, moving = true) => views.push(moving ? end : undefined) },
  }
  const pan = () => views.at(-1)
  let setRedraws: ((redraws: number) => void) | undefined
  function Chart() {
    const [element, setElement] = useState<HTMLDivElement | null>(null)
    ;[, setRedraws] = useState(0)
    // A fresh scroll each draw, as a page gives when new figures or prices arrive.
    const scroll: GraphScroll = { today: '2026-09-30', first: '2026-01-01' }
    useChartPan(element, week, scroll, { chart: handle })
    return <div ref={setElement} />
  }
  const { host, root, render } = mount(<Chart />)
  await render()
  const chart = host.querySelector('div')!
  // 700 pixels show the week, so each 100 scrolled is a day.
  Object.defineProperty(chart, 'clientWidth', { value: 700 })

  await act(async () => {
    wheelOn(chart, 75)
    wheelOn(chart, 75)
  })
  // A day and a half back from 6 pm, which the page doesn't hear while the scroll moves.
  expect(pan()).toBe(now - 1.5 * day)
  await act(async () => vi.advanceTimersByTime(100))
  expect(sessionGraphEnd()).toBeUndefined()
  await act(async () => setRedraws!(1))
  // Nothing lost to the redraw.
  await act(async () => wheelOn(chart, 150))
  expect(pan()).toBe(now - 3 * day)

  // Paused, it eases onto the nearest day instead of jumping there.
  await act(async () => vi.advanceTimersByTime(160 + 16 * 3))
  expect(pan()).toBeLessThan(now - 3 * day)
  expect(pan()).toBeGreaterThan(chartDayTime('2026-09-27'))
  await act(async () => vi.advanceTimersByTime(250))
  // It reaches the day, and once the page ends there too the chart draws at its own end.
  expect(views.at(-2)).toBe(chartDayTime('2026-09-27'))
  expect(pan()).toBeUndefined()
  expect(sessionGraphEnd()).toBe('2026-09-27')

  // Momentum the page started scrolling with isn't the chart's to take.
  const drawn = views.length
  await act(async () => wheelOn(chart, 300, false))
  await frame()
  expect(views).toHaveLength(drawn)
  await act(async () => root.unmount())
})

it('slides a chart along its own timeline and settles it on the nearest session', async () => {
  const views: Array<number | undefined> = []
  const handle = {
    current: { view: (end?: number, moving = true) => views.push(moving ? end : undefined) },
  }
  // A display day for each session, the weekend closed.
  const sessions = ['2026-09-24', '2026-09-25', '2026-09-28', '2026-09-29']
  const closed = Date.parse('2026-09-26T00:00:00Z') / 1_000
  const toDisplay = (time: number) => (time < closed ? time : Math.max(closed, time - 2 * day))
  const end = (session: string | undefined) => (session ? chartDayTime(session) + 43_199 : now)
  const timeline: ChartTimeline = {
    span: 5 * day,
    toDisplay,
    toActual: (time) => (time < closed ? time : time + 2 * day),
    end,
    day: (time) =>
      sessions.findLast(
        (session) => Math.abs(toDisplay(end(session)) - toDisplay(time)) <= day / 2,
      ),
  }
  function Chart() {
    const [element, setElement] = useState<HTMLDivElement | null>(null)
    useChartPan(
      element,
      week,
      { today: '2026-09-30', first: '2026-01-01' },
      { chart: handle, timeline },
    )
    return <div ref={setElement} />
  }
  const { host, root, render } = mount(<Chart />)
  await render()
  const chart = host.querySelector('div')!
  // 500 pixels show five sessions, so each 100 scrolled is one.
  Object.defineProperty(chart, 'clientWidth', { value: 500 })
  await act(async () => wheelOn(chart, 50))
  // Half a session, drawn as it is.
  expect(views.at(-1)).toBe(toDisplay(now) - day / 2)
  await act(async () => wheelOn(chart, 200))
  expect(views.at(-1)).toBe(toDisplay(now) - 2.5 * day)
  expect(sessionGraphEnd()).toBeUndefined()
  await act(async () => vi.advanceTimersByTime(160 + 250))
  // Across the weekend, onto the session nearest where it paused.
  expect(views.at(-2)).toBe(toDisplay(end('2026-09-25')))
  expect(views.at(-1)).toBeUndefined()
  expect(sessionGraphEnd()).toBe('2026-09-25')
  await act(async () => root.unmount())
})

it('scrolls a month back without a page commit until it settles', async () => {
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
  expect(pageCommits).toBe(0)
  await act(async () => vi.advanceTimersByTime(1_000))
  expect(sessionGraphEnd()).toBe('2026-08-31')
  console.info(`30 days on M: ${pageCommits} page commits, ${commits} commits`)
  // The settled day, and the figures asked for it.
  expect(pageCommits).toBeLessThanOrEqual(2)
  await act(async () => root.unmount())
})

it('scrolls a value chart a month back with the chart drawing each frame outside React', async () => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  )
  vi.stubGlobal('matchMedia', () => ({
    matches: false,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }))
  onTestFinished(() => {
    vi.unstubAllGlobals()
  })
  const original = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, 'getContext')!
  Object.defineProperty(HTMLCanvasElement.prototype, 'getContext', {
    configurable: true,
    value: () => null,
  })
  onTestFinished(() => {
    Object.defineProperty(HTMLCanvasElement.prototype, 'getContext', original)
  })
  const scroll: GraphScroll = { today: '2026-09-30', first: '2026-01-01' }
  const data = Array.from({ length: 273 }, (_, index) => ({
    time: chartDayTime(addDays('2026-01-01', index)),
    value: 1_000 + index,
  }))
  let commits = 0
  let chartRenders = 0
  function Page() {
    useGraphEnd()
    return (
      <Profiler id="chart" onRender={() => (chartRenders += 1)}>
        <PerformanceChart
          data={data}
          value={1_272}
          referenceIso="2026-09-30T18:00:00Z"
          selectedWindow={month}
          scroll={scroll}
        />
      </Profiler>
    )
  }
  const { host, root, render } = mount(
    <Profiler id="page" onRender={() => (commits += 1)}>
      <Page />
    </Profiler>,
  )
  await render()
  const chart = host.querySelector('.performance-chart')!
  Object.defineProperty(chart, 'clientWidth', { value: 700 })
  commits = 0
  chartRenders = 0
  for (let index = 0; index < 70; index += 1) {
    await act(async () => wheelOn(chart, 10))
    await frame()
  }
  // Once, as the scroll starts.
  expect(commits).toBe(1)
  await act(async () => vi.advanceTimersByTime(1_000))
  expect(sessionGraphEnd()).toBe('2026-08-31')
  console.info(`30 days of a value chart on M: ${commits} commits, ${chartRenders} chart renders`)
  expect(commits).toBeLessThanOrEqual(2)
  await act(async () => root.unmount())
})
