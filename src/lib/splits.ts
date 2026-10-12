import { cents } from './money'

export type SplitDraft = { category: string; amount: string }

const parse = (text: string) => {
  const value = Number(text.replaceAll(/[$,\s]/g, ''))
  return text.trim() && Number.isFinite(value) && value > 0 ? cents(value) : undefined
}

/**
 * Whether a draft divides a transaction: at least two named parts with positive amounts
 * that add up to its size. Parts are typed as magnitudes and saved with the transaction's sign.
 */
export function splitDraftState(amount: number, parts: SplitDraft[]) {
  const total = Math.abs(cents(amount))
  const values = parts.map((part) => parse(part.amount))
  const entered = values.reduce<number>((sum, value) => sum + (value ?? 0), 0)
  const complete =
    parts.length >= 2 &&
    parts.every((part) => part.category.trim()) &&
    values.every((value) => value !== undefined)
  return {
    remaining: (total - entered) / 100,
    valid: complete && entered === total,
    splits: parts.map((part, index) => ({
      category: part.category.trim(),
      amount: ((values[index] ?? 0) * Math.sign(amount)) / 100,
    })),
  }
}

/** Equal parts in cents; the leftover cents go to the first parts so the sum is exact. */
export function evenSplit(amount: number, count: number) {
  const total = Math.abs(cents(amount))
  const base = Math.floor(total / count)
  return Array.from({ length: count }, (_, index) =>
    ((base + (index < total - base * count ? 1 : 0)) / 100).toFixed(2),
  )
}
