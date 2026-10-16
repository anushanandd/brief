// @vitest-environment jsdom

import { act, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, expect, it, vi } from 'vitest'

import { useActiveSelectionScroll } from './use-active-selection-scroll'

function Selector() {
  const [selection, setSelection] = useState('first')
  const ref = useActiveSelectionScroll()

  return (
    <nav ref={ref}>
      <button aria-current={selection === 'first' ? 'page' : undefined}>First</button>
      <button
        aria-current={selection === 'second' ? 'page' : undefined}
        onClick={() => setSelection('second')}
      >
        Second
      </button>
    </nav>
  )
}

afterEach(() => {
  document.body.replaceChildren()
})

const rect = (left: number, right: number) => new DOMRect(left, 0, right - left, 0)

it('scrolls the strip, not the page, to keep the active selection visible', async () => {
  const scrollIntoView = vi.fn()
  Element.prototype.scrollIntoView = scrollIntoView
  const container = document.body.appendChild(document.createElement('div'))
  const root = createRoot(container)

  await act(async () => root.render(<Selector />))
  const nav = container.querySelector('nav')!
  const [first, second] = container.querySelectorAll('button')
  // A 100px strip holding two 80px items: the second starts out of view.
  Object.defineProperty(nav, 'scrollWidth', { value: 160 })
  Object.defineProperty(nav, 'clientWidth', { value: 100 })
  nav.getBoundingClientRect = () => rect(0, 100)
  first.getBoundingClientRect = () => rect(0, 80)
  second.getBoundingClientRect = () => rect(80, 160)

  await act(async () => second.click())
  expect(nav.scrollLeft).toBe(60)
  // The page is never scrolled to reach a strip below the fold.
  expect(scrollIntoView).not.toHaveBeenCalled()
  await act(async () => root.unmount())
})
