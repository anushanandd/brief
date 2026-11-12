import { Select } from '@base-ui/react/select'
import { Fragment, useRef } from 'react'

import { Check, ChevronDown } from './icons'
import { usePopupPortal } from './ui/popup-container'

/** A choice; options sharing a `group` in a row are listed under its name. */
export type FilterSelectOption = { value: string; label: string; group?: string }

/** Splits options into consecutive runs with the same group (or none). */
function optionRuns(options: FilterSelectOption[]) {
  const runs: Array<{ group?: string; options: FilterSelectOption[] }> = []
  for (const option of options) {
    const last = runs.at(-1)
    if (last && last.group === option.group) last.options.push(option)
    else runs.push({ group: option.group, options: [option] })
  }
  return runs
}

function FilterSelectItem({ option }: { option: FilterSelectOption }) {
  return (
    <Select.Item className="ledger-select-item" value={option.value}>
      <Select.ItemText className="ledger-select-item-text">{option.label}</Select.ItemText>
      <Select.ItemIndicator className="ledger-select-indicator">
        <Check size={12} aria-hidden="true" />
      </Select.ItemIndicator>
    </Select.Item>
  )
}

export function FilterSelect({
  label,
  value,
  options,
  onValueChange,
  className,
  command,
  focusRegionOnPointerSelection = false,
  disabled = false,
  keyboardOpen = false,
}: {
  label: string
  /** Inside a list row, Return on the row opens this choice. */
  keyboardOpen?: boolean
  focusRegionOnPointerSelection?: boolean
  className?: string
  /** The registered command (or command family) that makes this choice too. */
  command?: string
  disabled?: boolean
  value: string
  options: FilterSelectOption[]
  onValueChange: (value: string) => void
}) {
  const { container, positionMethod } = usePopupPortal()
  const triggerRef = useRef<HTMLButtonElement>(null)
  const closeReason = useRef<Select.Root.ChangeEventReason | undefined>(undefined)
  const pointerSelection = useRef(false)
  return (
    <Select.Root
      disabled={disabled}
      items={options}
      value={value}
      onOpenChange={(open, details) => {
        closeReason.current = open ? undefined : details.reason
        if (open) pointerSelection.current = false
      }}
      onValueChange={(nextValue) => {
        if (nextValue != null) onValueChange(nextValue)
      }}
    >
      <Select.Trigger
        ref={triggerRef}
        className={`ledger-select-trigger${className ? ` ${className}` : ''}`}
        aria-label={label}
        data-command={command}
        data-keyboard-open={keyboardOpen || undefined}
      >
        <Select.Value className="ledger-select-value">
          {options.find((option) => option.value === value)?.label}
        </Select.Value>
        <Select.Icon className="ledger-select-icon">
          <ChevronDown size={12} aria-hidden="true" />
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal container={container}>
        <Select.Positioner
          className="ledger-select-positioner"
          sideOffset={6}
          alignItemWithTrigger={false}
          positionMethod={positionMethod}
        >
          <Select.Popup
            className="ledger-select-popup"
            onPointerDown={() => {
              pointerSelection.current = true
            }}
            onKeyDown={() => {
              pointerSelection.current = false
            }}
            finalFocus={() => {
              if (closeReason.current === 'outside-press') return false
              if (
                !pointerSelection.current ||
                closeReason.current !== 'item-press' ||
                !focusRegionOnPointerSelection
              )
                return true
              const trigger = triggerRef.current
              return (
                trigger?.closest<HTMLElement>('[data-range-focus-region]') ??
                trigger
                  ?.closest('.surface-card')
                  ?.querySelector<HTMLElement>('[data-range-focus-region]') ??
                true
              )
            }}
          >
            <Select.List className="ledger-select-list">
              {optionRuns(options).map(({ group, options: run }, index) =>
                group ? (
                  <Select.Group className="ledger-select-group" key={index}>
                    <Select.GroupLabel className="ledger-select-group-label">
                      {group}
                    </Select.GroupLabel>
                    {run.map((option) => (
                      <FilterSelectItem option={option} key={option.value} />
                    ))}
                  </Select.Group>
                ) : (
                  <Fragment key={index}>
                    {run.map((option) => (
                      <FilterSelectItem option={option} key={option.value} />
                    ))}
                  </Fragment>
                ),
              )}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  )
}
