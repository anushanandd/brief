import { useLayoutEffect, useRef } from 'react'

export function viewportScrollHeight(top: number, windowHeight: number, bottomInset: number) {
  return Math.max(120, windowHeight - top - bottomInset)
}

export function useViewportScroll<T extends HTMLElement = HTMLDivElement>(bottomInset = 36) {
  const ref = useRef<T>(null)

  useLayoutEffect(() => {
    const element = ref.current
    if (!element) return undefined

    let frame = 0
    const update = () => {
      element.style.maxHeight = `${viewportScrollHeight(
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
  })

  return ref
}
