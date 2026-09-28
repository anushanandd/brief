import { BriefcaseBusiness, CircleEllipsis, Globe2, Store } from './icons'

export function ActivityMethodMark({ method }: { method: string }) {
  const Icon =
    method === 'Brokerage'
      ? BriefcaseBusiness
      : method === 'In store'
        ? Store
        : method === 'Online'
          ? Globe2
          : CircleEllipsis
  const color =
    method === 'Brokerage'
      ? 'var(--data-3)'
      : method === 'In store'
        ? 'var(--data-4)'
        : method === 'Online'
          ? 'var(--data-1)'
          : 'var(--data-2)'
  return (
    <span
      className="transaction-mark activity-method-mark"
      style={{ color, backgroundColor: `color-mix(in srgb, ${color} 14%, var(--surface-raised))` }}
      aria-hidden="true"
    >
      <Icon size={15} />
    </span>
  )
}
