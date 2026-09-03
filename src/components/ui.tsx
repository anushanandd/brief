import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react'

import { formatPercent } from '../lib/format'

export function Button({
  className,
  variant = 'secondary',
  size = 'default',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'secondary' | 'ghost'
  size?: 'default' | 'icon' | 'compact'
}) {
  return (
    <button
      className={`button-base button-${variant} button-${size}${className ? ` ${className}` : ''}`}
      {...props}
    />
  )
}

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <section className={`surface-card${className ? ` ${className}` : ''}`} {...props} />
}

export function SectionHeading({
  title,
  detail,
  action,
}: {
  title: string
  detail?: string
  action?: ReactNode
}) {
  return (
    <div className="section-heading">
      <div>
        <h2>{title}</h2>
        {detail ? <p>{detail}</p> : null}
      </div>
      {action}
    </div>
  )
}

export function Change({ value, label }: { value: number; label?: string }) {
  const tone = value > 0 ? 'positive' : value < 0 ? 'negative' : 'muted'

  return (
    <span className={`change ${tone}`}>
      {formatPercent(value)} {label}
    </span>
  )
}

export function StatusDot({ tone = 'positive' }: { tone?: string }) {
  return <span aria-hidden="true" className={`status-dot ${tone}`} />
}
