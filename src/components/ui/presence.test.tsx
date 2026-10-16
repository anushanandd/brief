// @vitest-environment jsdom
import { act, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { expect, it, vi } from 'vitest'

import { usePresence } from './presence'

function Panel({ open }: { open: boolean }) {
  const [element, setElement] = useState<HTMLDivElement | null>(null)
  const present = usePresence(open, element)
  return present ? <div ref={setElement} data-panel data-leaving={open ? undefined : ''} /> : null
}

it('keeps content mounted through its exit transition, then removes it once', async () => {
  // A running exit transition, as WebKit reports one, that finishes when the test says.
  let finishExit!: () => void
  const exit = new Promise<void>((resolve) => {
    finishExit = resolve
  })
  vi.stubGlobal('requestAnimationFrame', (run: FrameRequestCallback) => setTimeout(run, 0))
  vi.stubGlobal('cancelAnimationFrame', clearTimeout)
  Object.defineProperty(Element.prototype, 'getAnimations', {
    configurable: true,
    value: () => [{ playState: 'running', finished: exit }],
  })
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)

  await act(async () => root.render(<Panel open />))
  const panel = container.querySelector('[data-panel]')
  // Closing used to unmount and remount the content in a loop until React gave up.
  await act(async () => root.render(<Panel open={false} />))
  expect(container.querySelector('[data-panel]')).toBe(panel)
  expect(panel?.hasAttribute('data-leaving')).toBe(true)
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
    finishExit()
    await exit
  })
  expect(container.querySelector('[data-panel]')).toBeNull()

  await act(async () => root.unmount())
  container.remove()
  vi.unstubAllGlobals()
  Reflect.deleteProperty(Element.prototype, 'getAnimations')
})
