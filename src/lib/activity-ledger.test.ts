import { describe, expect, it } from 'vitest'

import seed from '../data/seed.json'
import { activityLedger, activityRows, type LedgerSearch } from './activity-ledger'
import { financeSnapshotSchema } from './schema'

const data = financeSnapshotSchema.parse(seed)
const ledger = (search: LedgerSearch, draft = '') =>
  activityLedger({
    data,
    rows: activityRows(data, search),
    search,
    draft,
    sort: { column: 'date', direction: 'desc' },
    today: '2026-08-31',
  })

describe('activity ledger', () => {
  it('filters by category and counts each filter without its own choice', () => {
    const all = ledger({})
    const dining = ledger({ category: 'Dining' })
    expect(dining.filtered.length).toBeGreaterThan(0)
    expect(dining.filtered.every((activity) => activity.category === 'Dining')).toBe(true)
    // Choosing Dining leaves every category's count as it was, so each says what it would show.
    expect(dining.options.categories).toEqual(all.options.categories)
    expect(dining.activeFilterCount).toBe(1)
    expect(dining.cardFilterCount).toBe(1)
  })

  it('removes each chip with the address that drops only that filter', () => {
    const view = ledger({ category: 'Dining,Groceries', min: 10, q: 'coffee' })
    expect(view.chips.map(({ id, patch }) => [id, patch])).toEqual([
      ['category:Dining', { category: 'Groceries' }],
      ['category:Groceries', { category: 'Dining' }],
      ['amount', { min: undefined, max: undefined }],
      ['text', { q: undefined }],
    ])
    // Text is a filter of its own, but the card counts only the filters it sets.
    expect(view.activeFilterCount).toBe(4)
    expect(view.cardFilterCount).toBe(3)
  })

  it('builds breadcrumbs that keep every filter before them and names the view', () => {
    const view = ledger({ category: 'Dining', from: '2026-08-01', to: '2026-08-31', max: 40 })
    const [category, dates] = view.breadcrumbs
    expect(category).toEqual({ label: 'Dining', search: { category: 'Dining' } })
    expect(dates.search).toEqual({ category: 'Dining', from: '2026-08-01', to: '2026-08-31' })
    expect(view.viewName).toBe(`Dining · ${dates.label} · Under $40.00`)
  })

  it('commits a draft on top of the committed filters', () => {
    // A finished word ends with a space; the word being typed only completes.
    const view = ledger({ category: 'Dining' }, 'groceries over $20 ')
    expect(view.preview).toEqual(['Groceries', 'Over $20.00'])
    expect(view.committed()).toMatchObject({
      category: 'Dining,Groceries',
      min: 20,
      q: undefined,
    })
  })

  it('offers All and Clear only where they change something', () => {
    const none = ledger({})
    expect(none.sections.categories.canClear).toBe(false)
    expect(none.sections.amount.canClear).toBe(false)
    const some = ledger({ category: 'Dining', min: 5 })
    expect(some.sections.categories).toMatchObject({ canAll: true, canClear: true })
    expect(some.sections.amount).toMatchObject({ canClear: true })
  })

  it('narrows to a cell by its value', () => {
    const view = ledger({})
    const activity = view.filtered.find(({ category }) => category === 'Groceries')!
    expect(view.cellFilter('category', activity)).toEqual({ category: 'Groceries' })
    expect(view.cellFilter('description', activity)).toEqual({ q: activity.title })
    expect(view.cellFilter('date', activity)).toMatchObject({ from: expect.any(String) })
  })
})
