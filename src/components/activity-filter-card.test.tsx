import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { ActivityFilterCard } from './activity-filter-card'

const noop = () => undefined
const categories = Array.from({ length: 13 }, (_, index) => ({
  value: `Category ${index + 1}`,
  label: `Category ${index + 1}`,
  count: 13 - index,
}))

const render = (overrides: Partial<Parameters<typeof ActivityFilterCard>[0]> = {}) =>
  renderToStaticMarkup(
    <ActivityFilterCard
      active={2}
      owner="other"
      ownerCounts={{ all: 12, me: 8, other: 4 }}
      onOwner={noop}
      datePresets={[
        { value: 'm', short: 'M', label: 'Month' },
        { value: 'a', short: 'All', label: 'All time' },
      ]}
      dateRange="custom"
      from="2026-09-01"
      to="2026-09-15"
      dateBounds={{ min: '2026-08-01', max: '2026-09-18' }}
      onDatePreset={noop}
      onDates={noop}
      categories={categories}
      included={[]}
      excluded={['Category 12']}
      onInclude={noop}
      accounts={[{ value: 'card', label: 'Synthetic card', count: 7 }]}
      selectedAccounts={['card']}
      onAccount={noop}
      methods={[
        { value: 'Online', label: 'Online', count: 5 },
        { value: 'In store', label: 'In store', count: 3 },
      ]}
      selectedMethods={[]}
      onMethod={noop}
      onAmount={noop}
      sections={{
        categories: { command: 'activity.filters.categories', clear: noop, all: noop },
        accounts: { command: 'activity.filters.accounts', clear: noop },
        methods: { command: 'activity.filters.methods', all: noop },
        amount: { command: 'activity.filters.amount' },
      }}
      onClear={noop}
      onClose={noop}
      {...overrides}
    />,
  )

it('lays short choices side by side and keeps every option a keyboard row', () => {
  const html = render()
  // For, date, and method are segments; each is its own keyboard row.
  expect(html).toMatch(
    /aria-label="For"[^>]*>(?:<button[^>]*data-keyboard-row[^>]*>[^<]*<\/button>){3}/,
  )
  expect(html).toContain('aria-label="Other, 4 activities"')
  expect(html).toMatch(/aria-pressed="true"[^>]*>Custom<\/button>/)
  expect(html).toContain('title="Month"')
  // A custom range is a slider over the saved activity, showing its two ends.
  expect(html).toContain('ledger-date-slider')
  expect(html).toContain('Sep 15, 2026')
})

it('shows the top categories, keeps a hidden one in view, and offers the rest', () => {
  const html = render()
  for (const shown of ['Category 1', 'Category 10', 'Category 12'])
    expect(html).toContain(`>${shown}<`)
  expect(html).not.toContain('>Category 11<')
  expect(html).toMatch(/data-hidden="true"[^]*?Category 12[^]*?Hidden/)
  expect(html).not.toContain('aria-label="Hide')
  expect(html).toContain('Show 2 more')
})

it('puts each section’s All and Clear beside its title as registered commands', () => {
  const html = render()
  expect(html).toContain('data-command="activity.filters.categories.all"')
  expect(html).toContain('data-command="activity.filters.categories.clear"')
  expect(html).toContain('data-command="activity.filters.accounts.clear"')
  expect(html).not.toContain('data-command="activity.filters.accounts.all"')
  expect(html).not.toContain('activity.filters.amount.clear')
  expect(html).toContain('aria-keyshortcuts="Escape F"')
  // One method is no choice, so its section is left out.
  expect(render({ methods: [{ value: 'Online', label: 'Online', count: 5 }] })).not.toContain(
    '>Method<',
  )
})
