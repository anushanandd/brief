import { useEffect, useRef } from 'react'

/**
 * Keeps the current item of a horizontal strip in view by scrolling the strip itself, never
 * the page: a strip below the fold must not pull the page down to it.
 */
export function useActiveSelectionScroll() {
  const ref = useRef<HTMLElement>(null)
  const previous = useRef<HTMLElement>(null)

  useEffect(() => {
    const root = ref.current
    const active = root?.querySelector<HTMLElement>('[aria-current="page"]')
    if (!root || !active || active === previous.current) return
    previous.current = active
    let strip: HTMLElement | null = active.parentElement
    while (strip && strip.scrollWidth <= strip.clientWidth)
      strip = strip === root ? null : strip.parentElement
    if (!strip) return
    const item = active.getBoundingClientRect()
    const view = strip.getBoundingClientRect()
    if (item.left < view.left) strip.scrollLeft -= view.left - item.left
    else if (item.right > view.right) strip.scrollLeft += item.right - view.right
  })

  return ref
}
