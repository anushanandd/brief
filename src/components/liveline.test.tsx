// @vitest-environment jsdom
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { Liveline, type LivelineHandle, type LivelineProps } from 'liveline'
import { act, createRef, Profiler } from 'react'
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

async function mount(props: LivelineProps, onRender = () => undefined) {
  const host = document.body.appendChild(document.createElement('div'))
  const root = createRoot(host)
  const render = (next: LivelineProps) =>
    act(async () =>
      root.render(
        <Profiler id="chart" onRender={onRender}>
          <Liveline {...next} />
        </Profiler>,
      ),
    )
  await render(props)
  onTestFinished(() => act(async () => root.unmount()))
  const chart = host.querySelector<HTMLElement>(':scope > div:last-child')!
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

const endingAt = (last: number) => [
  { time: 1_000, value: 100 },
  { time: 1_100, value: 200 },
  { time: 1_200, value: last },
]

it('eases a new value all the way, badge included, though the range stays put', async () => {
  const props = { ...settled, endTime: 1_200, window: 300, badge: true }
  const moved = await mount({ ...props, data: endingAt(150), value: 150 })
  await settle()
  await moved.render({ ...props, data: endingAt(160), value: 160 })
  await settle()
  const fresh = await mount({ ...props, data: endingAt(160), value: 160 })
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

const line = Array.from({ length: 11 }, (_, index) => ({
  time: 1_000 + index * 100,
  value: 100 + index * 10,
}))

it('moves its window to a handle’s end each frame without a render, and back to its own', async () => {
  const chart = createRef<LivelineHandle>()
  const onHover = vi.fn()
  let renders = 0
  const props = {
    ...settled,
    ref: chart,
    data: line,
    value: 200,
    endTime: 2_000,
    window: 400,
    badge: true,
    onHover,
    markers: [
      { id: 'before', time: 1_400, value: 140, color: '#fff', label: 'Before' },
      { id: 'after', time: 1_800, value: 180, color: '#fff', label: 'After' },
    ],
  }
  const { badge, marker: find } = await mount(props, () => {
    renders += 1
  })
  const marker = (label: string) => find(label)!
  await settle()
  expect(badge().textContent).toBe('200.00')
  expect(marker('After').style.display).toBe('block')
  await act(async () => marker('After').focus())
  await settle()
  renders = 0

  chart.current!.view(1_550)
  await frame()
  // The value at the end, at once, with the line ending there and what's after it hidden.
  expect(badge().textContent).toBe('155.00')
  expect(marker('Before').style.display).toBe('block')
  expect(marker('After').style.display).toBe('none')
  onHover.mockClear()
  await settle()
  expect(onHover).not.toHaveBeenCalledWith(expect.objectContaining({ time: 1_800 }))
  for (const end of [1_500, 1_450, 1_400]) {
    chart.current!.view(end)
    await frame()
  }
  expect(badge().textContent).toBe('140.00')
  // Once, to let go of the marker it hid.
  expect(renders).toBe(1)

  chart.current!.view(undefined, false)
  await settle()
  expect(badge().textContent).toBe('200.00')
})

it('reads every line at a settled end its data runs past', async () => {
  const props = {
    ...settled,
    data: line,
    value: 200,
    series: [
      { id: 'Value', data: line, value: 200, color: '#fff' },
      {
        id: 'Deposits',
        data: line.map(({ time }) => ({ time, value: 100 })),
        value: 100,
        color: '#fff',
      },
    ],
    endTime: 1_550,
    window: 400,
    badge: true,
  }
  const { badge } = await mount(props)
  await settle()
  expect(badge().textContent).toBe('155.00')
})

it('shows a marker only inside the plot, alone or beside other lines', async () => {
  for (const series of [undefined, [{ id: 'Value', data: line, value: 200, color: '#fff' }]]) {
    const { marker } = await mount({
      ...settled,
      data: line,
      value: 200,
      series,
      endTime: 2_000,
      window: 400,
      badge: true,
      markers: [
        { id: 'inside', time: 1_700, value: 170, color: '#fff', label: 'Inside plot' },
        // Rendered a window early, so a scroll finds it there, but hidden until it's drawn.
        { id: 'outside', time: 1_500, value: 150, color: '#fff', label: 'Outside plot' },
      ],
    })
    await settle()
    expect(marker('Inside plot')!.style.display).toBe('block')
    expect(marker('Outside plot')!.style.display).toBe('none')
  }
})

it('moves a marker by transform, and leaves one it hid alone until it shows', async () => {
  const chart = createRef<LivelineHandle>()
  const { marker } = await mount({
    ...settled,
    ref: chart,
    data: line,
    value: 200,
    endTime: 2_000,
    window: 400,
    markers: [
      { id: 'inside', time: 1_900, value: 190, color: '#fff', label: 'Inside plot' },
      { id: 'outside', time: 1_500, value: 150, color: '#fff', label: 'Outside plot' },
    ],
  })
  await settle()
  const inside = marker('Inside plot')!
  const outside = marker('Outside plot')!
  expect(inside.style.transform).toMatch(/^translate\(/)
  const placed = inside.style.transform
  const styles: CSSStyleDeclaration = Object.getPrototypeOf(inside.style)
  const display = vi.spyOn(styles, 'display', 'set')
  const transform = vi.spyOn(styles, 'transform', 'set')
  const layout = (['left', 'top'] as const).map((side) => vi.spyOn(styles, side, 'set'))
  for (const end of [1_990, 1_980, 1_970]) {
    chart.current!.view(end)
    await frame()
  }
  for (const side of layout) expect(side).not.toHaveBeenCalled()
  const written = [...display.mock.contexts, ...transform.mock.contexts]
  expect(written).not.toContain(outside.style)
  // Shown all along, so only its place is written.
  expect(display.mock.contexts).not.toContain(inside.style)
  expect(inside.style.transform).not.toBe(placed)
})

it('renders the markers a moving view reaches without the page rendering', async () => {
  const chart = createRef<LivelineHandle>()
  const far = { id: 'far', time: 400, value: 100, color: '#fff', label: 'Far back' }
  const { marker } = await mount({
    ...settled,
    ref: chart,
    data: [{ time: 0, value: 100 }, ...line],
    value: 200,
    endTime: 2_000,
    window: 400,
    markers: [far],
  })
  expect(marker('Far back')).toBeNull()
  for (const end of [1_600, 1_200, 800, 500]) {
    chart.current!.view(end)
    await frame()
  }
  await settle()
  expect(marker('Far back')?.style.display).toBe('block')
})

it('hides only Liveline’s own toolbar, so the holding chart’s session overlay shows', async () => {
  const style = document.head.appendChild(document.createElement('style'))
  style.textContent = readFileSync(join(process.cwd(), 'src/styles/components.css'), 'utf8')
  onTestFinished(() => style.remove())
  const host = document.body.appendChild(document.createElement('div'))
  host.className = 'live-performance-chart'
  const root = createRoot(host)
  onTestFinished(() => act(async () => root.unmount()))
  await act(async () =>
    root.render(
      <>
        <div className="holding-session-overlay" />
        <Liveline
          {...settled}
          className="liveline-chart-canvas"
          data={line}
          value={200}
          // Two lines bring out Liveline's series toggles.
          series={[
            { id: 'Value', data: line, value: 200, color: '#fff' },
            { id: 'Deposits', data: line, value: 200, color: '#fff' },
          ]}
          window={400}
        />
      </>,
    ),
  )
  const [overlay, toolbar, chart] = [...host.children]
  expect(getComputedStyle(toolbar!).display).toBe('none')
  expect(getComputedStyle(overlay!).display).not.toBe('none')
  expect(getComputedStyle(chart!).display).not.toBe('none')
})
