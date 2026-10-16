import type { SortState } from '../lib/sort'

/** A table's column headings, each sorting by its column; `command` prefixes each column id. */
export function SortHeadings<Column extends string>({
  columns,
  sort,
  command,
  onSort,
}: {
  columns: ReadonlyArray<{ id: Column; label: string }>
  sort: SortState<Column> | undefined
  command: string
  onSort?: (column: Column) => void
}) {
  return columns.map(({ id, label }) => {
    const active = sort?.column === id
    return (
      <button
        key={id}
        type="button"
        data-column={id}
        data-command={`${command}${id}`}
        onClick={() => onSort?.(id)}
        disabled={!onSort}
        aria-label={`Sort by ${label}${active ? `, ${sort.direction}ending` : ''}`}
        aria-pressed={active}
      >
        <span>{label}</span>
        {active ? <span aria-hidden="true">{sort.direction === 'asc' ? '↑' : '↓'}</span> : null}
      </button>
    )
  })
}
