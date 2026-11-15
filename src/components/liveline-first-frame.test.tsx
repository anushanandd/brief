// @vitest-environment jsdom
import { Liveline } from 'liveline'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, expect, it, onTestFinished, vi } from 'vitest'

afterEach(() => {
  vi.useRealTimers()
  vi.restoreAllMocks()
  document.body.replaceChildren()
})

it('draws a chart mounted with data before its first frame, so no blank canvas paints', async () => {
  vi.useFakeTimers({ toFake: ['requestAnimationFrame', 'cancelAnimationFrame'] })
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
  let drawCalls = 0
  const context: object = new Proxy(
    {},
    {
      get: (_, key) =>
        key === 'measureText'
          ? () => ({ width: 10 })
          : key === 'createLinearGradient' || key === 'createRadialGradient'
            ? () => ({ addColorStop: () => undefined })
            : () => {
                drawCalls += 1
              },
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
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () =>
    root.render(
      <Liveline
        data={[
          { time: 1_000, value: 100 },
          { time: 1_060, value: 101 },
        ]}
        value={101}
        endTime={1_060}
        window={120}
        continuous={false}
      />,
    ),
  )
  expect(drawCalls).toBeGreaterThan(0)
  await act(async () => root.unmount())
})
