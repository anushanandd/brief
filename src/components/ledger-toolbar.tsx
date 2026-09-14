import { Search, X } from 'lucide-react'
import type { ReactNode, Ref } from 'react'

import { Button } from './ui'

export function LedgerToolbar({
  label,
  placeholder,
  value,
  onValueChange,
  inputRef,
  children,
}: {
  label: string
  placeholder: string
  value: string
  onValueChange: (value: string) => void
  inputRef?: Ref<HTMLInputElement>
  children?: ReactNode
}) {
  return (
    <div className="spending-history-toolbar">
      <label className="spending-history-search">
        <Search size={16} aria-hidden="true" />
        <span className="sr-only">{label}</span>
        <input
          ref={inputRef}
          type="search"
          value={value}
          onChange={(event) => onValueChange(event.target.value)}
          placeholder={placeholder}
        />
        {value ? (
          <Button
            size="icon"
            variant="ghost"
            onClick={() => onValueChange('')}
            aria-label="Clear search"
          >
            <X size={14} aria-hidden="true" />
          </Button>
        ) : null}
      </label>
      {children ? <div className="ledger-filters">{children}</div> : null}
    </div>
  )
}
