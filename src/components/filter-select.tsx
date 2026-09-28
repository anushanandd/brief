import { Select } from '@base-ui/react/select'
import { useRef } from 'react'

import { Check, ChevronDown } from './icons'

export type FilterSelectOption = { value: string; label: string }

export function FilterSelect({
  label,
  value,
  options,
  onValueChange,
  className,
  blurOnClose = false,
  disabled = false,
}: {
  label: string
  blurOnClose?: boolean
  className?: string
  disabled?: boolean
  value: string
  options: FilterSelectOption[]
  onValueChange: (value: string) => void
}) {
  const triggerRef = useRef<HTMLButtonElement>(null)
  return (
    <Select.Root
      disabled={disabled}
      items={options}
      value={value}
      onOpenChangeComplete={(open) => {
        if (!open && blurOnClose) {
          // Base UI restores trigger focus in an unmount microtask.
          setTimeout(() => triggerRef.current?.blur())
        }
      }}
      onValueChange={(nextValue) => {
        if (nextValue != null) onValueChange(nextValue)
      }}
    >
      <Select.Trigger
        ref={triggerRef}
        className={`ledger-select-trigger${className ? ` ${className}` : ''}`}
        aria-label={label}
      >
        <Select.Value className="ledger-select-value">
          {options.find((option) => option.value === value)?.label}
        </Select.Value>
        <Select.Icon className="ledger-select-icon">
          <ChevronDown size={13} aria-hidden="true" />
        </Select.Icon>
      </Select.Trigger>
      <Select.Portal>
        <Select.Positioner
          className="ledger-select-positioner"
          sideOffset={6}
          alignItemWithTrigger={false}
        >
          <Select.Popup className="ledger-select-popup">
            <Select.List className="ledger-select-list">
              {options.map((option) => (
                <Select.Item className="ledger-select-item" key={option.value} value={option.value}>
                  <Select.ItemIndicator className="ledger-select-indicator">
                    <Check size={13} aria-hidden="true" />
                  </Select.ItemIndicator>
                  <Select.ItemText className="ledger-select-item-text">
                    {option.label}
                  </Select.ItemText>
                </Select.Item>
              ))}
            </Select.List>
          </Select.Popup>
        </Select.Positioner>
      </Select.Portal>
    </Select.Root>
  )
}
