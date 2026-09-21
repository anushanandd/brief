import { ChevronDown, X } from 'lucide-react'
import { useEffect, useState, type ReactNode } from 'react'

import { Button, Card, SectionHeading } from './ui'

export const selectedFilterValues = (query: string | undefined, options: string[]) =>
  (query ?? '').split(',').filter((value) => options.includes(value))

export function toggleFilterValue(values: string[], value: string) {
  if (!value) return values
  return values.includes(value) ? values.filter((item) => item !== value) : [...values, value]
}

export function LedgerFilters({
  activeCount,
  onClearAll,
  children,
}: {
  activeCount: number
  onClearAll: () => void
  children: ReactNode
}) {
  const [expanded, setExpanded] = useState(activeCount > 0)
  useEffect(() => {
    if (activeCount > 0) setExpanded(true)
  }, [activeCount])

  return (
    <Card className="ledger-filter-card" data-expanded={expanded || undefined}>
      <div className="ledger-filter-header">
        <SectionHeading title="Filters" />
        <div className="ledger-filter-actions">
          {activeCount ? (
            <button
              className="ledger-filter-clear ledger-filter-clear-all"
              type="button"
              onClick={onClearAll}
            >
              <X size={14} aria-hidden="true" />
              <span className="sr-only">Clear all</span>
            </button>
          ) : null}
          <Button
            size="icon"
            variant="ghost"
            type="button"
            className="ledger-filter-toggle"
            aria-label={`${expanded ? 'Collapse' : 'Expand'} filters${activeCount ? `, ${activeCount} active` : ''}`}
            aria-expanded={expanded}
            onClick={() => setExpanded((current) => !current)}
          >
            <ChevronDown size={16} aria-hidden="true" />
          </Button>
        </div>
      </div>
      <div className="ledger-filter-body">{children}</div>
    </Card>
  )
}

export function FilterCheckboxGroup({
  label,
  options,
  values,
  onToggle,
  onClear,
}: {
  label: string
  options: { value: string; label: string }[]
  values: Set<string>
  onToggle: (value: string) => void
  onClear: () => void
}) {
  return (
    <fieldset className="ledger-checkbox-group">
      <legend className="sr-only">{label}</legend>
      {values.size ? (
        <div className="ledger-checkbox-group-header">
          <button
            type="button"
            className="ledger-filter-clear"
            aria-label={`Clear ${label.toLowerCase()}`}
            onClick={onClear}
          >
            <X size={14} aria-hidden="true" />
            <span className="sr-only">Clear</span>
          </button>
        </div>
      ) : null}
      {options.map((option) => (
        <label key={option.value}>
          <input
            type="checkbox"
            checked={values.has(option.value)}
            onChange={() => onToggle(option.value)}
          />
          <span>{option.label}</span>
        </label>
      ))}
    </fieldset>
  )
}
