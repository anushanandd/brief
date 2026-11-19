// @vitest-environment jsdom

import { afterEach, describe, expect, it, vi } from 'vitest'

import { adjacentRowIndex, contextualListActions } from './keyboard-regions'

describe('list motions', () => {
  it('steps one row and wraps at either end', () => {
    expect(adjacentRowIndex(0, 10, 1)).toBe(1)
    expect(adjacentRowIndex(0, 10, -1)).toBe(9)
    expect(adjacentRowIndex(9, 10, 1)).toBe(0)
    expect(adjacentRowIndex(-1, 4, 1)).toBe(0)
    expect(adjacentRowIndex(0, 0, 1)).toBe(-1)
  })
})

function opened(selects: boolean, motion = 'motion.next') {
  // jsdom lays nothing out, so every row counts as visible.
  vi.spyOn(HTMLElement.prototype, 'getClientRects').mockReturnValue(
    Object.assign([new DOMRect(0, 0, 10, 10)], { item: () => null }),
  )
  HTMLElement.prototype.scrollIntoView = () => undefined
  document.body.innerHTML = `<nav aria-label="Menu" data-keyboard-region ${selects ? 'data-keyboard-selects' : ''}>
      <a href="#a" data-keyboard-row data-keyboard-open>A</a>
      <a href="#b" data-keyboard-row data-keyboard-open>B</a>
    </nav>`
  const region = document.querySelector<HTMLElement>('nav')!
  const [first, second] = region.querySelectorAll('a')
  first!.focus()
  const clicks: string[] = []
  second!.addEventListener('click', (event) => {
    event.preventDefault()
    clicks.push('b')
  })
  contextualListActions(region)
    .find(({ id }) => id.startsWith(motion))!
    .run()
  return { focused: document.activeElement === second, clicks }
}

describe('menus', () => {
  afterEach(() => {
    vi.restoreAllMocks()
    document.body.replaceChildren()
  })

  it('shows the row a menu steps to, and only moves focus in other lists', () => {
    expect(opened(true)).toEqual({ focused: true, clicks: ['b'] })
    expect(opened(false)).toEqual({ focused: true, clicks: [] })
    expect(opened(true, 'motion.last')).toEqual({ focused: true, clicks: ['b'] })
  })
})
