import { useLayoutEffect, useRef, useState } from 'react'

export function viewportScrollHeight(top: number, windowHeight: number, bottomInset: number) {
  return Math.max(120, windowHeight - top - bottomInset)
}

/**
 * Keeps an element reaching the foot of the window: a list's most height, so it scrolls past
 * it, or with `minHeight` a region's least, so it fills the window when its content is short.
 */
export function useViewportScroll<T extends HTMLElement = HTMLDivElement>(
  bottomInset = 36,
  property: 'maxHeight' | 'minHeight' = 'maxHeight',
) {
  const ref = useRef<T>(null)
  const [element, setElement] = useState<T | null>(null)

  // The element can mount after the hook, as when a page renders its loading state first.
  useLayoutEffect(() => {
    if (ref.current !== element) setElement(ref.current)
  })

  useLayoutEffect(() => {
    if (!element) return undefined

    let frame = 0
    const update = () => {
      element.style[property] = `${viewportScrollHeight(
        element.getBoundingClientRect().top,
        window.innerHeight,
        bottomInset,
      )}px`
    }
    const schedule = () => {
      if (frame) return
      frame = requestAnimationFrame(() => {
        frame = 0
        update()
      })
    }

    update()
    window.addEventListener('resize', schedule)
    window.addEventListener('scroll', schedule, { passive: true })
    const observer = new ResizeObserver(schedule)
    if (element.parentElement) observer.observe(element.parentElement)

    return () => {
      window.removeEventListener('resize', schedule)
      window.removeEventListener('scroll', schedule)
      observer.disconnect()
      cancelAnimationFrame(frame)
    }
  }, [element, bottomInset, property])

  return ref
}
