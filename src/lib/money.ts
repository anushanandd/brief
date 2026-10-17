/** Whole cents from dollars. Halves round up, as Rust's `numeric::minor_units` does. */
export const cents = (dollars: number) => Math.round(dollars * 100)

/** Dollars summed in whole cents, so float error never reaches a total. */
export const sumCents = (values: Iterable<number>) => {
  let total = 0
  for (const value of values) total += cents(value)
  return total / 100
}
