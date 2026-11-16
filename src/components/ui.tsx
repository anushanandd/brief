import NumberFlow from '@number-flow/react'
import { useLayoutEffect, useRef } from 'react'
import type { ComponentPropsWithRef, ReactNode } from 'react'

import {
  amountsHidden,
  formatCurrency,
  formatPercent,
  formatPublicCurrency,
  formatSignedCurrency,
  valueTone,
} from '../lib/format'
import { useGraphScrolling } from '../lib/graph-preferences'
import { FilterSelect } from './filter-select'
import type { IconComponent } from './icons'

export function AnimatedCurrency({
  value,
  className,
  publicValue = false,
  still = false,
}: {
  value: number | null | undefined
  className?: string
  /** A public market price, which screen privacy leaves visible. */
  publicValue?: boolean
  /** Plain text, for a figure that changes every frame. */
  still?: boolean
}) {
  const scrolling = useGraphScrolling()
  // NumberFlow draws its own digits, so a hidden amount never reaches it.
  if (still || value == null || (amountsHidden() && !publicValue))
    return (
      <PlainCurrency
        className={className}
        text={(publicValue ? formatPublicCurrency : formatCurrency)(value)}
      />
    )
  return (
    // The sign sits outside the digits: Brief writes a true minus, and Intl a hyphen.
    <FlowNumber
      className={className}
      value={Math.abs(value)}
      animated={scrolling}
      prefix={sign(value, '')}
      format={{ style: 'currency', currency: 'USD', maximumFractionDigits: 2 }}
    />
  )
}

/** Formatted currency split into NumberFlow's parts, so styles for its parts apply to both. */
function PlainCurrency({ className, text }: { className?: string; text: string }) {
  const parts = /^(.*?)(\$)([\d,•]+)(\.\d+)?$/.exec(text)
  if (!parts) return <span className={className}>{text}</span>
  const [, minus, currency, integer, fraction] = parts
  return (
    <span className={className}>
      {minus}
      <span className="currency">{currency}</span>
      <span className="integer">{integer}</span>
      {fraction ? <span className="fraction">{fraction}</span> : null}
    </span>
  )
}

const sign = (value: number, plus: string) => (value < 0 ? '−' : value > 0 ? plus : '')
const percentFormat = {
  style: 'percent',
  minimumFractionDigits: 2,
  maximumFractionDigits: 2,
} as const
const percentFormatter = new Intl.NumberFormat('en-US', percentFormat)

/**
 * A figure in NumberFlow's own digits. Values change when data arrives or a key steps through
 * rows, which update in place; only a scroll through time, a gesture the eye follows, rolls them.
 */
function FlowNumber(props: Omit<ComponentPropsWithRef<typeof NumberFlow>, 'locales'>) {
  return (
    <NumberFlow
      locales="en-US"
      respectMotionPreference
      transformTiming={{
        duration: 200,
        easing: 'cubic-bezier(0.77, 0, 0.175, 1)',
      }}
      opacityTiming={{
        duration: 120,
        easing: 'cubic-bezier(0.23, 1, 0.32, 1)',
      }}
      {...props}
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

  // The region fades out at its bottom edge while more sits below, as macOS lists do; a chevron
  // over the last row read as a stray control.
  useLayoutEffect(() => {
    const card = ref.current
    if (!card) return undefined
    const scrollRegion = card.querySelector<HTMLElement>(scrollSelector)
    if (!scrollRegion) return undefined
    const update = () => {
      if (hasMoreBelow(scrollRegion)) scrollRegion.dataset.moreBelow = ''
      else delete scrollRegion.dataset.moreBelow
    }
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
  still = false,
}: {
  amount: number | null | undefined
  percent: number | null | undefined
  favorable?: 'increase' | 'decrease'
  ariaLabel?: string
  /** Plain text, for a change that moves every frame. */
  still?: boolean
}) {
  const tone = valueTone(amount == null || favorable === 'increase' ? amount : -amount)
  const scrolling = useGraphScrolling()

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
      {still || amount == null || amountsHidden() ? (
        <span className={tone}>{formatSignedCurrency(amount)}</span>
      ) : (
        // The sign sits outside the digits: Brief writes a true minus, and Intl a hyphen.
        <FlowNumber
          className={tone}
          value={Math.abs(amount)}
          animated={scrolling}
          prefix={sign(amount, '+')}
          format={{
            style: 'currency',
            currency: 'USD',
            minimumFractionDigits: 2,
            maximumFractionDigits: 2,
          }}
        />
      )}
      {percent == null ? null : still ? (
        <span className={tone}>
          {`(${sign(percent, '')}${percentFormatter.format(Math.abs(percent) / 100)})`}
        </span>
      ) : (
        <FlowNumber
          className={tone}
          value={Math.abs(percent) / 100}
          animated={scrolling}
          prefix={`(${sign(percent, '')}`}
          suffix=")"
          format={percentFormat}
        />
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
