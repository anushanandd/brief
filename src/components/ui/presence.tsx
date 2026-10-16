import { useEffect, useRef, useState, type ReactNode } from 'react'

/**
 * Keeps content around while its exit transition plays, so what slid in leaves the same way.
 * Returns `value` while it is set, then the last value until `element`'s own transitions end,
 * then nothing. Without the Web Animations API (test DOMs) the content leaves at once.
 */
export function usePresence<T>(value: T | false | null | undefined, element: Element | null) {
  const last = useRef<T | undefined>(undefined)
  if (value) last.current = value
  const open = Boolean(value)
  // Leaving starts in the render that closes, so the content stays mounted through it. Marked
  // later, the content would unmount and remount with every close and never settle.
  const [wasOpen, setWasOpen] = useState(open)
  const [leaving, setLeaving] = useState(false)
  if (open !== wasOpen) {
    setWasOpen(open)
    setLeaving(!open)
  }

  useEffect(() => {
    if (!leaving) return undefined
    if (!element || typeof element.getAnimations !== 'function') {
      setLeaving(false)
      return undefined
    }
    let cancelled = false
    const leave = () => {
      if (!cancelled) setLeaving(false)
    }
    // Transitions start at the next style flush, so read them after it. The element's own
    // transitions are enough: a child's looping animation would never finish.
    const frame = requestAnimationFrame(() => {
      const running = element
        .getAnimations()
        .filter((animation) => animation.playState !== 'finished')
      if (!running.length) return leave()
      void Promise.allSettled(running.map((animation) => animation.finished)).then(leave)
    })
    return () => {
      cancelled = true
      cancelAnimationFrame(frame)
    }
  }, [leaving, element])

  if (value) return value
  return leaving ? last.current : undefined
}

/**
 * A disclosed block that fades in where it opens and leaves at once. Groups open and close many
 * times a day, often from the keyboard, so closing never waits: a fade out would hold the rows'
 * space until it ended and then drop everything below in one late jump.
 */
export function DisclosedPanel({
  open,
  id,
  className,
  children,
}: {
  open: boolean
  id?: string
  className: string
  children: ReactNode
}) {
  if (!open) return null
  return (
    <div id={id} className={className}>
      {children}
    </div>
  )
}
