import { useEffect, useState } from 'react'

import { currentKeyboardRegion } from '../lib/keyboard-regions'

/**
 * The keyboard region holding focus, so list shortcuts follow it. Focus moving within a region,
 * or between controls outside any region, keeps the same value and doesn't render.
 */
export function useFocusRegion() {
  const [region, setRegion] = useState<HTMLElement>()
  useEffect(() => {
    const update = () => setRegion(currentKeyboardRegion())
    document.addEventListener('focusin', update)
    return () => document.removeEventListener('focusin', update)
  }, [])
  return region
}
