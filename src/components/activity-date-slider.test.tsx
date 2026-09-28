// @vitest-environment jsdom

import { act, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { ActivityDateSlider } from './activity-date-slider'

it('keeps the start thumb focused across keyboard date commits', async () => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
  Object.defineProperty(globalThis, 'IS_REACT_ACT_ENVIRONMENT', {
    value: true,
    configurable: true,
  })
  const onChange = vi.fn()
  function Harness() {
    const [range, setRange] = useState({ from: '2026-09-10', to: '2026-09-20' })
    return (
      <ActivityDateSlider
        minDate="2026-09-01"
        maxDate="2026-09-30"
        from={range.from}
        to={range.to}
        onChange={(from, to) => {
          onChange(from, to)
          setRange({ from, to })
        }}
      />
    )
  }
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  await act(async () => root.render(<Harness />))
  const start = container.querySelector<HTMLInputElement>('input[type="range"]')!
  await act(async () => {
    start.focus()
    start.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
  })
  expect(onChange).toHaveBeenLastCalledWith('2026-09-11', '2026-09-20')
  expect(document.activeElement).toBe(start)
  await act(async () => {
    start.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }))
  })
  expect(onChange).toHaveBeenLastCalledWith('2026-09-12', '2026-09-20')
  expect(document.activeElement).toBe(start)
  await act(async () => root.unmount())
  container.remove()
  vi.unstubAllGlobals()
})
