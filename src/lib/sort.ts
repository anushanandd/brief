export type SortDirection = 'asc' | 'desc'
export type SortState<Column extends string> = { column: Column; direction: SortDirection }

/** The sort after choosing a column: the sorted one reverses; another starts at `firstDirection`. */
export function nextSort<Column extends string>(
  current: SortState<Column>,
  column: Column,
  firstDirection: SortDirection,
): SortState<Column> {
  if (current.column !== column) return { column, direction: firstDirection }
  return { column, direction: current.direction === 'asc' ? 'desc' : 'asc' }
}
