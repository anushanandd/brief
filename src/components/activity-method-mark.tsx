import type { activityPlace } from '../lib/activity'
import {
  ArrowLeftRight,
  BriefcaseBusiness,
  CircleEllipsis,
  Globe2,
  MapPinned,
  Store,
} from './icons'

type PlaceKind = ReturnType<typeof activityPlace>['kind']

const placeStyles = {
  place: { Icon: MapPinned, color: 'var(--data-5)' },
  online: { Icon: Globe2, color: 'var(--data-1)' },
  store: { Icon: Store, color: 'var(--data-4)' },
  brokerage: { Icon: BriefcaseBusiness, color: 'var(--data-3)' },
  transfer: { Icon: ArrowLeftRight, color: 'var(--chart-secondary)' },
  none: { Icon: CircleEllipsis, color: 'var(--data-2)' },
} as const

/** Where or how a purchase happened, colored like the category marks beside it. */
export function PlaceMark({ kind }: { kind: PlaceKind }) {
  const { Icon, color } = placeStyles[kind]
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

export function ActivityMethodMark({ method }: { method: string }) {
  return (
    <PlaceMark
      kind={
        method === 'Brokerage'
          ? 'brokerage'
          : method === 'In store'
            ? 'store'
            : method === 'Online'
              ? 'online'
              : 'none'
      }
    />
  )
}
