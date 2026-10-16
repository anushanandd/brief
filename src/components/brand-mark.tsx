import { useEffect, useState, type CSSProperties, type ReactNode } from 'react'

import { useLogoCrop } from '../lib/logo-crop'

export function BrandMark({
  className,
  fallback,
  label,
  src,
  style,
}: {
  className: string
  fallback: ReactNode
  label: string
  src?: string
  style?: CSSProperties
}) {
  const [failed, setFailed] = useState(false)
  // A logo drawn on a white margin is zoomed so its artwork fills the circle.
  const crop = useLogoCrop(failed ? undefined : src)

  useEffect(() => setFailed(false), [src])

  return (
    <span className={`${className} brand-mark`} style={style}>
      {src && !failed ? (
        <img
          src={src}
          alt=""
          aria-hidden="true"
          loading="lazy"
          onError={() => setFailed(true)}
          style={
            crop
              ? { transform: `translate(${crop.x}%, ${crop.y}%) scale(${crop.scale})` }
              : undefined
          }
        />
      ) : (
        <span aria-hidden="true">{fallback}</span>
      )}
      <span className="sr-only">{label}</span>
    </span>
  )
}
