import { Select } from '@base-ui/react/select'
import { Check, ChevronsUpDown } from 'lucide-react'

export type FilterSelectOption = { value: string; label: string }

export function FilterSelect({
  label,
  value,
  options,
  onValueChange,
  disabled = false,
}: {
  label: string
  disabled?: boolean
  value: string
  options: FilterSelectOption[]
  onValueChange: (value: string) => void
}) {
  return (
    <Select.Root
      disabled={disabled}
      items={options}
      value={value}
      onValueChange={(nextValue) => {
        if (nextValue != null) onValueChange(nextValue)
      }}
    >
      <Select.Trigger className="ledger-select-trigger" aria-label={label}>
        <Select.Value className="ledger-select-value">
          {options.find((option) => option.value === value)?.label}
        </Select.Value>
        <Select.Icon className="ledger-select-icon">
          <ChevronsUpDown size={13} aria-hidden="true" />
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
