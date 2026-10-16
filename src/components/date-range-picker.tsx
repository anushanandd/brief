import { Popover } from '@base-ui/react/popover'
import { useState } from 'react'

import { CalendarDays, Check, ChevronDown } from './icons'
import { Keys } from './keys'
import { usePopupPortal } from './ui/popup-container'

export type DateRangePreset<T extends string> = {
  value: T
  label: string
  /** The page shortcut that picks this preset, shown beside it. */
  shortcut?: string
}

/**
 * Date presets in one popover. Choosing one closes it. Exact dates are typed in search, such as
 * "aug 1 to sep 2" or "since 6/1", and the trigger then reads as that range.
 */
export function DateRangePicker<T extends string>({
  label,
  command,
  presets,
  value,
  summary,
  onPreset,
}: {
  label: string
  /** The registered command family this picker's presets also run as. */
  command?: string
  presets: DateRangePreset<T>[]
  /** The matching preset, or undefined for a typed range. */
  value: T | undefined
  /** What the trigger shows, such as "Last 30 days" or "Aug 1 – Aug 15". */
  summary: string
  onPreset: (value: T) => void
}) {
  const { container, positionMethod } = usePopupPortal()
  const [open, setOpen] = useState(false)
  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger
        className="date-range-trigger"
        aria-label={`${label}: ${summary}`}
        data-command={command}
        data-custom={value === undefined || undefined}
      >
        <CalendarDays size={15} aria-hidden="true" />
        <span>{summary}</span>
        <ChevronDown size={14} aria-hidden="true" />
      </Popover.Trigger>
      <Popover.Portal container={container}>
        <Popover.Positioner
          side="bottom"
          align="end"
          sideOffset={6}
          positionMethod={positionMethod}
          className="date-range-positioner"
        >
          <Popover.Popup className="date-range-popup" aria-label={label}>
            <div className="date-range-presets" role="group" aria-label="Presets">
              {presets.map((preset) => (
                <button
                  type="button"
                  key={preset.value}
                  aria-pressed={preset.value === value}
                  onClick={() => {
                    onPreset(preset.value)
                    setOpen(false)
                  }}
                >
                  <span>{preset.label}</span>
                  {preset.value === value ? (
                    <Check size={12} aria-hidden="true" />
                  ) : preset.shortcut ? (
                    <Keys label={preset.shortcut} />
                  ) : null}
                </button>
              ))}
            </div>
          </Popover.Popup>
        </Popover.Positioner>
      </Popover.Portal>
    </Popover.Root>
  )
}
