import { useEffect, useState, type CSSProperties, type ReactNode } from 'react'

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

  useEffect(() => setFailed(false), [src])

  return (
    <span className={`${className} brand-mark`} style={style}>
      {src && !failed ? (
        <img src={src} alt="" aria-hidden="true" loading="lazy" onError={() => setFailed(true)} />
      ) : (
        <span aria-hidden="true">{fallback}</span>
      )}
      <span className="sr-only">{label}</span>
    </span>
  )
}
