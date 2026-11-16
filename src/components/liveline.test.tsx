// @vitest-environment jsdom
import { Liveline, type LivelineProps } from 'liveline'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, onTestFinished, vi } from 'vitest'

let calls: string[] = []

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame', 'performance'] })
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
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(
    new DOMRect(0, 0, 400, 200),
  )
  calls = []
  const context: object = new Proxy(
    {},
    {
      get: (_, key) =>
        key === 'measureText'
          ? () => ({ width: 10 })
          : key === 'createLinearGradient' || key === 'createRadialGradient'
            ? () => ({ addColorStop: () => undefined })
            : () => calls.push(String(key)),
      set: () => true,
    },
  )
  const original = Object.getOwnPropertyDescriptor(HTMLCanvasElement.prototype, 'getContext')!
  Object.defineProperty(HTMLCanvasElement.prototype, 'getContext', {
    configurable: true,
    value: () => context,
  })
  onTestFinished(() => {
    Object.defineProperty(HTMLCanvasElement.prototype, 'getContext', original)
  })
})
afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  document.body.replaceChildren()
})

const settled = { continuous: false, pulse: false, momentum: false } as const

async function mount(props: LivelineProps) {
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  const render = (next: LivelineProps) => act(async () => root.render(<Liveline {...next} />))
  await render(props)
  onTestFinished(() => act(async () => root.unmount()))
  const chart = document.body.lastElementChild!.lastElementChild as HTMLElement
  return {
    chart,
    render,
    badge: () => chart.querySelector<HTMLElement>(':scope > div')!,
    marker: (label: string) => chart.querySelector<HTMLElement>(`[aria-label="${label}"]`),
  }
}

const frame = () => act(async () => vi.advanceTimersToNextFrame())

/** Frames drawn until the chart stops asking for one, up to `limit`. */
async function settle(limit = 400) {
  let frames = 0
  while (frames < limit && vi.getTimerCount() > 0) {
    await frame()
    frames += 1
  }
  return frames
}

it('draws a chart mounted with data before its first frame, so no blank canvas paints', async () => {
  await mount({
    data: [
      { time: 1_000, value: 100 },
      { time: 1_060, value: 101 },
    ],
    value: 101,
    endTime: 1_060,
    window: 120,
    continuous: false,
  })
  expect(calls.length).toBeGreaterThan(0)
})

it('stops drawing a settled candle chart', async () => {
  await mount({
    ...settled,
    mode: 'candle',
    candles: [
      { time: 1_000, open: 100, high: 104, low: 99, close: 103 },
      { time: 1_060, open: 103, high: 106, low: 102, close: 105 },
    ],
    candleWidth: 60,
    data: [],
    value: 105,
    endTime: 1_120,
    window: 240,
  })
  expect(await settle()).toBeLessThan(400)
})

it('eases a new value all the way, badge included, though the range stays put', async () => {
  const data = (last: number) => [
    { time: 1_000, value: 100 },
    { time: 1_100, value: 200 },
    { time: 1_200, value: last },
  ]
  const props = { ...settled, endTime: 1_200, window: 300, badge: true }
  const moved = await mount({ ...props, data: data(150), value: 150 })
  await settle()
  await moved.render({ ...props, data: data(160), value: 160 })
  await settle()
  const fresh = await mount({ ...props, data: data(160), value: 160 })
  expect(moved.badge().textContent).toBe('160.00')
  expect(moved.badge().style.transform).toBe(fresh.badge().style.transform)
})

it('ends a series at its own last point when another runs on, and badges and reads that one', async () => {
  const onHover = vi.fn()
  const series = [
    {
      id: 'Value',
      data: [
        { time: 1_000, value: 100 },
        { time: 1_100, value: 110 },
      ],
      value: 110,
      color: '#fff',
    },
    {
      id: 'Estimated',
      data: [
        { time: 1_100, value: 110 },
        { time: 1_200, value: 130 },
      ],
      value: 130,
      color: '#fff',
      dash: [4, 4],
    },
  ]
  const props = { ...settled, data: [], value: 130, series, endTime: 1_200, window: 300, onHover }
  const { chart, badge, render } = await mount(props)
  await settle()
  expect(badge().textContent).toBe('130.00')
  calls = []
  await render({ ...props })
  await frame()
  // One end dot: the estimate's.
  expect(calls.filter((call) => call === 'arc')).toHaveLength(1)
  // 1,150 s sits 241 px into the 308 px plot that starts 12 px in.
  chart.dispatchEvent(new MouseEvent('mousemove', { clientX: 253 }))
  await frame()
  expect(onHover.mock.lastCall?.[0].value).toBeCloseTo(120, 0)
})

it('lets go of a marker once it is gone, though its button never blurred', async () => {
  const onHover = vi.fn()
  const props = {
    ...settled,
    data: [
      { time: 1_000, value: 100 },
      { time: 1_100, value: 110 },
      { time: 1_200, value: 120 },
    ],
    value: 120,
    endTime: 1_200,
    window: 300,
    onHover,
  }
  const markers = [{ id: 'saved', time: 1_100, value: 110, color: '#fff', label: 'Saved' }]
  const { marker, render } = await mount({ ...props, markers })
  await act(async () => marker('Saved')!.focus())
  await settle()
  expect(onHover).toHaveBeenLastCalledWith(expect.objectContaining({ time: 1_100 }))
  onHover.mockClear()
  await render({ ...props, markers: [] })
  await settle()
  expect(onHover).not.toHaveBeenCalledWith(expect.objectContaining({ time: 1_100 }))
})

it('plays the whole reveal when data arrives after an empty chart', async () => {
  const props = { ...settled, endTime: 1_200, window: 300, badge: true }
  const { badge, render } = await mount({ ...props, data: [], value: 0 })
  await settle()
  await render({
    ...props,
    data: [
      { time: 1_000, value: 100 },
      { time: 1_200, value: 120 },
    ],
    value: 120,
  })
  await frame()
  expect(badge().style.display).toBe('none')
  await settle()
  expect(badge().style.display).toBe('')
})
