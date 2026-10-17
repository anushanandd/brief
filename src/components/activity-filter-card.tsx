import { useEffect, useState, type ReactNode } from 'react'

import type { ActivityEvidence } from '../lib/activity'
import { evidenceKinds, evidenceLabels } from '../lib/activity-ledger'
import { ActivityDateSlider } from './activity-date-slider'
import { CategoryMark } from './category-mark'
import { Check, X } from './icons'
import { Button, Card, SectionHeading } from './ui'

export type FilterOption = { value: string; label: string; count: number }
type Owner = 'me' | 'other'
const evidenceShort: Record<ActivityEvidence, string> = {
  statement: 'Statement',
  unlisted: 'Not listed',
  conflict: 'Disagree',
  unsure: 'Unsure',
}
/** A section's own reset and select-all, each a registered command. */
type SectionActions = { clear?: () => void; all?: () => void; command: string }

/** Categories shown before "Show more"; chosen or hidden ones always show. */
const visibleCategories = 10

/**
 * Activity's filters as a companion card beside the ledger, opened with Filter or F and closed
 * with F or Esc. Every option is a keyboard row, so the list's motions move through it and Enter
 * chooses. A count is what choosing that option would show, given the other filters.
 */
export function ActivityFilterCard({
  active,
  owner,
  ownerCounts,
  onOwner,
  evidence,
  evidenceCounts,
  onEvidence,
  datePresets,
  dateRange,
  from,
  to,
  dateBounds,
  onDatePreset,
  onDates,
  categories,
  included,
  excluded,
  onInclude,
  accounts,
  selectedAccounts,
  onAccount,
  methods,
  selectedMethods,
  onMethod,
  min,
  max,
  onAmount,
  sections,
  onClear,
  onClose,
}: {
  active: number
  owner?: Owner
  ownerCounts: Record<'all' | Owner, number>
  onOwner: (owner?: Owner) => void
  evidence?: ActivityEvidence
  evidenceCounts?: Record<'all' | ActivityEvidence, number>
  onEvidence?: (evidence?: ActivityEvidence) => void
  /** Short labels (W, M, …) with their full names. */
  datePresets: ReadonlyArray<{ value: string; short: string; label: string }>
  /** The preset in effect, or `custom`. */
  dateRange: string
  from?: string
  to?: string
  /** The first and last days with activity, the slider's ends. */
  dateBounds?: { min: string; max: string }
  onDatePreset: (value: string) => void
  onDates: (from?: string, to?: string) => void
  categories: FilterOption[]
  included: readonly string[]
  excluded: readonly string[]
  onInclude: (category: string) => void
  accounts: FilterOption[]
  selectedAccounts: readonly string[]
  onAccount: (id: string) => void
  methods: FilterOption[]
  selectedMethods: readonly string[]
  onMethod: (method: string) => void
  min?: number
  max?: number
  onAmount: (min?: number, max?: number) => void
  sections: Record<'categories' | 'accounts' | 'methods' | 'amount', SectionActions>
  onClear: () => void
  onClose: () => void
}) {
  const [custom, setCustom] = useState(dateRange === 'custom')
  const [allCategories, setAllCategories] = useState(false)
  // A preset chosen elsewhere (the date button, a chip, Clear) closes the custom range.
  useEffect(() => {
    setCustom(dateRange === 'custom')
  }, [dateRange])
  const chosen = new Set([...included, ...excluded])
  const shownCategories = allCategories
    ? categories
    : categories.filter((option, index) => index < visibleCategories || chosen.has(option.value))
  const moreCategories = categories.length - shownCategories.length

  return (
    <Card className="activity-filter-card">
      <SectionHeading
        title="Filters"
        action={
          <>
            {active ? (
              <Button
                size="compact"
                variant="ghost"
                data-command="activity.filters.clear"
                onClick={onClear}
              >
                Clear {active}
              </Button>
            ) : null}
            <Button
              size="icon-compact"
              variant="ghost"
              icon={X}
              aria-label="Close filters"
              aria-keyshortcuts="Escape F"
              data-command="activity.filters.toggle"
              onClick={onClose}
            />
          </>
        }
      />
      <div
        className="activity-filter-body"
        role="group"
        aria-label="Filters"
        tabIndex={0}
        data-keyboard-region
      >
        <FilterSection title="For">
          <Segments
            label="For"
            options={[
              { value: undefined, short: 'Anyone', label: 'Anyone', count: ownerCounts.all },
              { value: 'me' as const, short: 'Me', label: 'Me', count: ownerCounts.me },
              { value: 'other' as const, short: 'Other', label: 'Other', count: ownerCounts.other },
            ]}
            selected={(value) => owner === value}
            onSelect={onOwner}
          />
        </FilterSection>

        {/* Only once Rust has graded something does the evidence filter have choices. */}
        {evidenceCounts &&
        onEvidence &&
        (evidence || evidenceKinds.some((kind) => evidenceCounts[kind])) ? (
          <FilterSection title="Evidence">
            <Segments
              label="Evidence"
              options={[
                { value: undefined, short: 'Any', label: 'Any', count: evidenceCounts.all },
                ...evidenceKinds.map((kind) => ({
                  value: kind,
                  short: evidenceShort[kind],
                  label: evidenceLabels[kind],
                  count: evidenceCounts[kind],
                })),
              ]}
              selected={(value) => evidence === value}
              onSelect={onEvidence}
            />
          </FilterSection>
        ) : null}

        <FilterSection title="Date">
          <Segments
            label="Date"
            options={[
              ...datePresets.map(({ value, short, label }) => ({ value, short, label })),
              { value: 'custom', short: 'Custom', label: 'Custom dates' },
            ]}
            selected={(value) => (value === 'custom' ? custom : !custom && dateRange === value)}
            onSelect={(value) => {
              setCustom(value === 'custom')
              if (value !== 'custom') onDatePreset(value)
            }}
          />
          {custom && dateBounds ? (
            <div className="activity-filter-dates">
              <ActivityDateSlider
                minDate={dateBounds.min}
                maxDate={dateBounds.max}
                from={from}
                to={to}
                onChange={onDates}
              />
            </div>
          ) : null}
        </FilterSection>

        {methods.length > 1 ? (
          <FilterSection title="Method" actions={sections.methods}>
            <Segments
              label="Method"
              multiple
              options={methods.map(({ value, label, count }) => ({
                value,
                short: label,
                label,
                count,
              }))}
              selected={(value) => selectedMethods.includes(value)}
              onSelect={onMethod}
            />
          </FilterSection>
        ) : null}

        <FilterSection title="Categories" actions={sections.categories}>
          {shownCategories.map(({ value, label, count }) => {
            const hidden = excluded.includes(value)
            return (
              <FilterRow
                key={value}
                label={label}
                count={count}
                mark={<CategoryMark category={value} />}
                selected={included.includes(value)}
                hidden={hidden}
                onSelect={() => onInclude(value)}
              />
            )
          })}
          {moreCategories > 0 || allCategories ? (
            <button
              type="button"
              className="activity-filter-more"
              data-keyboard-row
              data-keyboard-open
              aria-expanded={allCategories}
              onClick={() => setAllCategories((current) => !current)}
            >
              {allCategories ? 'Show fewer' : `Show ${moreCategories} more`}
            </button>
          ) : null}
        </FilterSection>

        <FilterSection title="Accounts" actions={sections.accounts}>
          {accounts.map(({ value, label, count }) => (
            <FilterRow
              key={value}
              label={label}
              count={count}
              selected={selectedAccounts.includes(value)}
              onSelect={() => onAccount(value)}
            />
          ))}
        </FilterSection>

        <FilterSection title="Amount" actions={sections.amount}>
          <AmountRange min={min} max={max} onAmount={onAmount} />
        </FilterSection>
      </div>
    </Card>
  )
}

function FilterSection({
  title,
  actions,
  children,
}: {
  title: string
  actions?: SectionActions
  children: ReactNode
}) {
  return (
    <section className="activity-filter-section" aria-label={title}>
      <header>
        <h3>{title}</h3>
        {actions?.all ? (
          <button
            type="button"
            data-command={`${actions.command}.all`}
            aria-label={`Select every ${title.toLocaleLowerCase()} option`}
            onClick={actions.all}
          >
            All
          </button>
        ) : null}
        {actions?.clear ? (
          <button
            type="button"
            data-command={`${actions.command}.clear`}
            aria-label={`Clear ${title.toLocaleLowerCase()} filters`}
            onClick={actions.clear}
          >
            Clear
          </button>
        ) : null}
      </header>
      {children}
    </section>
  )
}

/**
 * A short row of choices side by side. Each segment is its own keyboard row, so the list's
 * motions reach every one; `multiple` lets several be on at once.
 */
function Segments<T extends string | undefined>({
  label,
  options,
  selected,
  onSelect,
  multiple = false,
}: {
  label: string
  options: ReadonlyArray<{ value: T; short: string; label: string; count?: number }>
  selected: (value: T) => boolean
  onSelect: (value: T) => void
  multiple?: boolean
}) {
  return (
    <div className="activity-filter-segments" role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.label}
          type="button"
          data-keyboard-row
          data-keyboard-open
          // Pressed rather than a radio role: radios take the arrow keys, and here the arrows
          // move through every filter.
          aria-pressed={selected(option.value)}
          aria-label={
            option.count == null ? option.label : `${option.label}, ${option.count} activities`
          }
          title={multiple || option.short !== option.label ? option.label : undefined}
          onClick={() => onSelect(option.value)}
        >
          {option.short}
        </button>
      ))}
    </div>
  )
}

function FilterRow({
  label,
  count,
  mark,
  selected,
  hidden = false,
  onSelect,
}: {
  label: string
  count?: number
  mark?: ReactNode
  selected: boolean
  hidden?: boolean
  onSelect: () => void
}) {
  return (
    <div
      className="activity-filter-row"
      data-keyboard-row
      tabIndex={-1}
      data-hidden={hidden || undefined}
    >
      <button
        type="button"
        className="activity-filter-option"
        data-keyboard-open
        aria-pressed={selected}
        onClick={onSelect}
      >
        <span className="activity-filter-check" aria-hidden="true">
          {selected ? <Check size={11} /> : null}
        </span>
        {mark}
        <span className="activity-filter-label">{label}</span>
        {hidden ? (
          <span className="activity-filter-count">Hidden</span>
        ) : count != null ? (
          <span className="activity-filter-count">{count.toLocaleString('en-US')}</span>
        ) : null}
      </button>
    </div>
  )
}

const bound = (value: string) => {
  const amount = Number(value.replace(/[$,\s]/g, ''))
  return value.trim() && Number.isFinite(amount) && amount >= 0 ? amount : undefined
}

/** Typed bounds apply as they're typed; a blank or invalid side is unbounded. */
function AmountRange({
  min,
  max,
  onAmount,
}: {
  min?: number
  max?: number
  onAmount: (min?: number, max?: number) => void
}) {
  const [low, setLow] = useState(min?.toString() ?? '')
  const [high, setHigh] = useState(max?.toString() ?? '')
  // Bounds changed elsewhere (a chip, Clear) replace what's typed; matching ones leave it.
  useEffect(() => {
    if (bound(low) !== min) setLow(min?.toString() ?? '')
    if (bound(high) !== max) setHigh(max?.toString() ?? '')
  }, [min, max])
  return (
    <div className="activity-filter-range">
      <input
        inputMode="decimal"
        placeholder="Min"
        aria-label="Minimum amount"
        className="sensitive-value"
        value={low}
        onChange={(event) => {
          setLow(event.target.value)
          onAmount(bound(event.target.value), bound(high))
        }}
      />
      <span>to</span>
      <input
        inputMode="decimal"
        placeholder="Max"
        aria-label="Maximum amount"
        className="sensitive-value"
        value={high}
        onChange={(event) => {
          setHigh(event.target.value)
          onAmount(bound(low), bound(event.target.value))
        }}
      />
    </div>
  )
}
