// @vitest-environment jsdom

import { act, createElement } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { useViewportScroll, viewportScrollHeight } from './use-viewport-scroll'

it('keeps a list within the visible window while leaving a usable scroll area', () => {
  expect(viewportScrollHeight(300, 900, 36)).toBe(564)
  expect(viewportScrollHeight(820, 900, 36)).toBe(120)
})

function List({ loading, tick }: { loading: boolean; tick: number }) {
  const ref = useViewportScroll()
  return loading ? null : createElement('div', { ref, 'data-tick': tick })
}

it('sizes a list that mounts after the hook, and listens once across renders', () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      disconnect() {}
    },
  )
  const listen = vi.spyOn(window, 'addEventListener')
  const container = document.body.appendChild(document.createElement('div'))
  const root = createRoot(container)
  act(() => root.render(createElement(List, { loading: true, tick: 0 })))
  act(() => root.render(createElement(List, { loading: false, tick: 0 })))
  expect(container.querySelector('div')?.style.maxHeight).toMatch(/px$/)
  act(() => root.render(createElement(List, { loading: false, tick: 1 })))
  act(() => root.render(createElement(List, { loading: false, tick: 2 })))
  expect(listen.mock.calls.filter(([type]) => type === 'resize')).toHaveLength(1)
  act(() => root.unmount())
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})
