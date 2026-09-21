import NumberFlow from '@number-flow/react'
import type { LucideIcon } from 'lucide-react'
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react'

import { formatCurrency, formatPercent, valueTone } from '../lib/format'

export function AnimatedCurrency({
  value,
  className,
}: {
  value: number | null | undefined
  className?: string
}) {
  if (value == null) return <span className={className}>{formatCurrency(value)}</span>

  return (
    <NumberFlow
      className={className}
      value={value}
      locales="en-US"
      respectMotionPreference
      format={{ style: 'currency', currency: 'USD', maximumFractionDigits: 2 }}
      transformTiming={{
        duration: 200,
        easing: 'cubic-bezier(0.77, 0, 0.175, 1)',
      }}
      opacityTiming={{
        duration: 120,
        easing: 'cubic-bezier(0.23, 1, 0.32, 1)',
      }}
    />
  )
}

export function Button({
  className,
  variant = 'secondary',
  size = 'default',
  title,
  icon: Icon,
  children,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  icon?: LucideIcon
  variant?: 'primary' | 'secondary' | 'ghost' | 'destructive'
  size?: 'default' | 'icon' | 'icon-compact' | 'compact'
}) {
  return (
    <button
      className={`button-base button-${variant} button-${size}${className ? ` ${className}` : ''}`}
      aria-label={props['aria-label'] ?? title}
      {...props}
    >
      {Icon ? (
        <>
          <Icon size={16} aria-hidden="true" />
          <span className="sr-only">{children}</span>
        </>
      ) : (
        children
      )}
    </button>
  )
}

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <div className={`surface-card${className ? ` ${className}` : ''}`} {...props} />
}

export function EmptyState({
  children,
  title,
  action,
}: {
  children?: ReactNode
  title?: string
  action?: ReactNode
}) {
  return (
    <div className="empty-state" role="status">
      {title ? <strong>{title}</strong> : null}
      {children ? <p>{children}</p> : null}
      {action}
    </div>
  )
}

export function Metric({
  label,
  value,
  detail,
  tone,
}: {
  label: ReactNode
  value: ReactNode
  detail?: ReactNode
  tone?: 'positive' | 'negative' | 'muted'
}) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong className={tone}>{value}</strong>
      {detail != null ? <small>{detail}</small> : null}
    </div>
  )
}

export function SectionHeading({
  title,
  detail,
  action,
}: {
  title: ReactNode
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

export type RangeSelectorOption<T extends number | string> = {
  value: T
  label: string
  accessibleLabel: string
}

export function RangeSelector<T extends number | string>({
  label,
  options,
  value,
  onValueChange,
  className,
}: {
  label: string
  options: RangeSelectorOption<T>[]
  value: T
  onValueChange: (value: T) => void
  className?: string
}) {
  return (
    <div
      className={`range-selector${className ? ` ${className}` : ''}`}
      role="group"
      aria-label={label}
    >
      {options.map((option) => (
        <Button
          key={option.value}
          type="button"
          size="compact"
          variant="ghost"
          aria-label={option.accessibleLabel}
          aria-pressed={option.value === value}
          title={option.accessibleLabel}
          onClick={() => onValueChange(option.value)}
        >
          {option.label}
        </Button>
      ))}
    </div>
  )
}

export function Change({
  value,
  label,
  favorable = 'increase',
}: {
  value: number | null | undefined
  label?: string
  favorable?: 'increase' | 'decrease'
}) {
  const signedValue = value == null ? value : favorable === 'decrease' ? -value : value
  const tone = valueTone(signedValue)

  return (
    <span className={`change ${tone}`}>
      {formatPercent(value)} {label}
    </span>
  )
}

export function StatusDot({ tone = 'positive' }: { tone?: string }) {
  return <span aria-hidden="true" className={`status-dot ${tone}`} />
}
