import { useLayoutEffect, useRef } from 'react'

/**
 * A ref holding the latest `value`, for stable callbacks (commands, listeners) that must run the
 * current handlers without re-registering. It updates after each render, never during one.
 */
export function useLatest<T>(value: T) {
  const ref = useRef(value)
  useLayoutEffect(() => {
    ref.current = value
  })
  return ref
}
