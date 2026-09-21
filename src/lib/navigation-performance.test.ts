import { afterEach, expect, it, vi } from 'vitest'

import { measureNavigation } from './navigation-performance'

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  performance.clearMeasures('brief:navigation-to-frame')
})

it('records completed visible switches after two frames and discards superseded navigation', () => {
  const listeners = new Map<
    string,
    (event: { fromLocation?: unknown; pathChanged: boolean }) => void
  >()
  const frames: FrameRequestCallback[] = []
  vi.stubGlobal('document', { hidden: false })
  vi.stubGlobal('requestAnimationFrame', (callback: FrameRequestCallback) => frames.push(callback))
  const now = vi.spyOn(performance, 'now').mockReturnValue(10)
  const dispose = measureNavigation({
    subscribe: (name, callback) => {
      listeners.set(name, callback)
      return () => listeners.delete(name)
    },
  })
  const navigate = () => listeners.get('onBeforeNavigate')!({ fromLocation: {}, pathChanged: true })
  const render = () => listeners.get('onRendered')!({ pathChanged: true })
  const frame = () => frames.shift()!(performance.now())
  navigate()
  render()
  frame()
  expect(performance.getEntriesByName('brief:navigation-to-frame')).toHaveLength(0)
  now.mockReturnValue(30)
  frame()
  expect(performance.getEntriesByName('brief:navigation-to-frame')[0].duration).toBe(20)
  navigate()
  render()
  navigate()
  frame()
  frame()
  expect(performance.getEntriesByName('brief:navigation-to-frame')).toHaveLength(1)
  render()
  vi.stubGlobal('document', { hidden: true })
  frame()
  frame()
  expect(performance.getEntriesByName('brief:navigation-to-frame')).toHaveLength(1)
  dispose()
  expect(listeners.size).toBe(0)
})
