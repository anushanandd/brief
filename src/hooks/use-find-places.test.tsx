// @vitest-environment jsdom

import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { useFindPlaces } from './use-find-places'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async () => {
    throw new Error('No event bridge')
  }),
}))
vi.mock('../lib/api', () => ({ findPlaces: vi.fn(async () => undefined) }))

it('stops running when it cannot listen for progress', async () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  let places!: ReturnType<typeof useFindPlaces>
  function Probe() {
    places = useFindPlaces()
    return null
  }
  const root = createRoot(document.createElement('div'))
  act(() => root.render(<Probe />))
  await act(async () => {
    await expect(places.start()).rejects.toThrow('No event bridge')
  })
  expect(places.running).toBe(false)
  act(() => root.unmount())
  vi.unstubAllGlobals()
})
