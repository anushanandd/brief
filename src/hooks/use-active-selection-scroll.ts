import { useEffect, useRef } from 'react'

export function useActiveSelectionScroll() {
  const ref = useRef<HTMLElement>(null)
  const previous = useRef<HTMLElement>(null)

  useEffect(() => {
    const active = ref.current?.querySelector<HTMLElement>('[aria-current="page"]')
    if (!active || active === previous.current) return
    previous.current = active
    active.scrollIntoView({ block: 'nearest', inline: 'nearest' })
  })

  return ref
}
