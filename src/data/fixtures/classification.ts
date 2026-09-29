import type { Transaction } from '../../lib/schema'

type Classification = Transaction['classification']

// Explicit native-result fixtures. Tests choose meaning; no merchant/category inference here.
export function classification(
  kind: Classification['kind'],
  overrides: Partial<Classification> = {},
): Classification {
  const mark =
    kind === 'expense' || kind === 'other' || kind === 'tax'
      ? 'initial'
      : kind === 'reimbursement'
        ? 'refund'
        : kind
  return {
    kind,
    mark,
    spending: kind === 'expense',
    credit: kind === 'reimbursement',
    zelle: false,
    ...overrides,
  }
}
