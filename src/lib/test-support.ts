/**
 * Test helper: the value, or a failure naming what was missing. Lets a test index into a
 * result under `noUncheckedIndexedAccess` without a non-null assertion, and fails with a
 * clear message instead of a TypeError further on.
 */
export function present<T>(value: T | null | undefined, what = 'value'): T {
  if (value === undefined || value === null) throw new Error(`Expected ${what}`)
  return value
}
