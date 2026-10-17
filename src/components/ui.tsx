import NumberFlow from '@number-flow/react'
import { useLayoutEffect, useRef, useState } from 'react'
import type { ComponentPropsWithRef, ReactNode } from 'react'

import { formatCurrency, formatPercent, formatSignedCurrency, valueTone } from '../lib/format'
import { FilterSelect } from './filter-select'
import { ChevronDown } from './icons'
import type { IconComponent } from './icons'

export function AnimatedCurrency({
  value,
  className,
}: {
  value: number | null | undefined
  className?: string
}) {
  if (value == null) return <span className={className}>{formatCurrency(value)}</span>

  // Values change when data arrives or a key steps through rows, never by a gesture worth
  // animating, so digits update in place rather than rolling.
  return (
    <NumberFlow
      className={className}
      value={value}
      locales="en-US"
      animated={false}
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
}: ComponentPropsWithRef<'button'> & {
  icon?: IconComponent
  variant?: 'primary' | 'secondary' | 'ghost' | 'positive' | 'destructive'
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
          {size === 'icon' || size === 'icon-compact' ? (
            <span className="sr-only">{children}</span>
          ) : (
            children
          )}
        </>
      ) : (
        children
      )}
    </button>
  )
}

export function hasMoreBelow({
  scrollHeight,
  clientHeight,
  scrollTop,
}: Pick<HTMLElement, 'scrollHeight' | 'clientHeight' | 'scrollTop'>) {
  return scrollHeight - clientHeight - scrollTop > 1
}

export function Card({ className, ...props }: ComponentPropsWithRef<'div'>) {
  return <div className={`surface-card${className ? ` ${className}` : ''}`} {...props} />
}

export function ScrollCueCard({
  className,
  children,
  scrollSelector,
  ...props
}: Omit<ComponentPropsWithRef<'div'>, 'ref'> & { scrollSelector: string }) {
  const ref = useRef<HTMLDivElement>(null)
  const [showScrollCue, setShowScrollCue] = useState(false)

  useLayoutEffect(() => {
    const card = ref.current
    if (!card) return undefined
    const scrollRegion = card.querySelector<HTMLElement>(scrollSelector)
    if (!scrollRegion) return undefined
    const update = () => setShowScrollCue(hasMoreBelow(scrollRegion))
    const observer = new ResizeObserver(update)
    observer.observe(card)
    observer.observe(scrollRegion)
    if (scrollRegion.firstElementChild) observer.observe(scrollRegion.firstElementChild)
    scrollRegion.addEventListener('scroll', update, { passive: true })
    update()
    return () => {
      observer.disconnect()
      scrollRegion.removeEventListener('scroll', update)
    }
  }, [children, scrollSelector])

  return (
    <Card ref={ref} className={className} {...props}>
      <span
        className="card-scroll-cue"
        data-visible={showScrollCue || undefined}
        aria-hidden="true"
      >
        <ChevronDown size={16} strokeWidth={1.75} />
      </span>
      {children}
    </Card>
  )
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
  tone,
}: {
  label: ReactNode
  value: ReactNode
  tone?: 'positive' | 'negative' | 'muted'
}) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong className={tone}>{value}</strong>
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
      {action == null ? null : <div className="section-heading-action">{action}</div>}
    </div>
  )
}

export type RangeSelectorOption<T extends number | string> = {
  value: T
  label: string
  accessibleLabel: string
}

export function ChartRangeSelect<T extends number | string>({
  label,
  command,
  options,
  value,
  onValueChange,
}: {
  label: string
  command?: string
  options: RangeSelectorOption<T>[]
  value: T
  onValueChange: (value: T) => void
}) {
  return (
    <FilterSelect
      focusRegionOnPointerSelection
      className="chart-range-select"
      command={command}
      label={label}
      value={String(value)}
      options={options.map((option) => ({
        value: String(option.value),
        label: option.accessibleLabel,
      }))}
      onValueChange={(nextValue) => {
        const option = options.find(({ value: optionValue }) => String(optionValue) === nextValue)
        if (option) onValueChange(option.value)
      }}
    />
  )
}

export function ChartChange({
  amount,
  percent,
  favorable = 'increase',
  ariaLabel,
}: {
  amount: number | null | undefined
  percent: number | null | undefined
  favorable?: 'increase' | 'decrease'
  ariaLabel?: string
}) {
  const tone = valueTone(amount == null || favorable === 'increase' ? amount : -amount)

  if (amount == null && percent == null)
    return (
      <strong
        className="hero-change hero-change-unavailable"
        role={ariaLabel ? 'group' : undefined}
        aria-label={ariaLabel}
      >
        Change unavailable
      </strong>
    )

  return (
    <strong className="hero-change" role={ariaLabel ? 'group' : undefined} aria-label={ariaLabel}>
      <span className={tone}>{formatSignedCurrency(amount)}</span>
      {percent == null ? null : (
        <span className={tone}>({formatPercent(percent).replace(/^\+/, '')})</span>
      )}
    </strong>
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
