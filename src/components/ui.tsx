import { cva, type VariantProps } from 'class-variance-authority'
import type { ButtonHTMLAttributes, HTMLAttributes, ReactNode } from 'react'

import { cn } from '../lib/cn'
import { formatPercent } from '../lib/format'

const buttonVariants = cva(
  'button-base inline-flex items-center justify-center gap-2 rounded-[10px] text-sm font-medium',
  {
    variants: {
      variant: {
        primary: 'button-primary',
        secondary: 'button-secondary',
        ghost: 'button-ghost',
      },
      size: {
        default: 'h-10 px-4',
        icon: 'size-10 p-0',
        compact: 'h-8 px-3 text-xs',
      },
    },
    defaultVariants: { variant: 'secondary', size: 'default' },
  },
)

export function Button({
  className,
  variant,
  size,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & VariantProps<typeof buttonVariants>) {
  return <button className={cn(buttonVariants({ variant, size }), className)} {...props} />
}

export function Card({ className, ...props }: HTMLAttributes<HTMLDivElement>) {
  return <section className={cn('surface-card', className)} {...props} />
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
    <span className={cn('change', tone)}>
      {formatPercent(value)} {label}
    </span>
  )
}

export function StatusDot({ tone = 'positive' }: { tone?: string }) {
  return <span aria-hidden="true" className={cn('status-dot', tone)} />
}

export function EmptyState({ title, description }: { title: string; description: string }) {
  return (
    <div className="empty-state">
      <div className="empty-state-mark" aria-hidden="true" />
      <h3>{title}</h3>
      <p>{description}</p>
    </div>
  )
}
