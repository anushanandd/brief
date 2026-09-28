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

it('keeps the active selection visible when it changes', async () => {
  const scrollIntoView = vi.fn()
  Element.prototype.scrollIntoView = scrollIntoView
  const container = document.body.appendChild(document.createElement('div'))
  const root = createRoot(container)

  await act(async () => root.render(<Selector />))
  await act(async () => container.querySelector<HTMLButtonElement>('button:last-child')!.click())

  expect(scrollIntoView).toHaveBeenLastCalledWith({ block: 'nearest', inline: 'nearest' })
  expect(scrollIntoView).toHaveBeenCalledTimes(2)
  await act(async () => root.unmount())
})
