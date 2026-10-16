import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { RankedBars } from './ranked-bars'

const items = [
  { name: 'Dining', value: 40, color: 'red', mark: 25, details: <p>Dining merchants</p> },
  { name: 'Travel', value: 10, color: 'blue', mark: null, details: <p>Travel merchants</p> },
]

it('selects one row for the page and shows only its details', () => {
  const html = renderToStaticMarkup(
    <RankedBars
      items={items}
      label="Spending by category"
      emptyMessage="None."
      selected="Dining"
      onSelect={() => undefined}
    />,
  )
  expect(html.match(/aria-pressed="true"/g)).toHaveLength(1)
  expect(html.match(/aria-pressed="false"/g)).toHaveLength(1)
  expect(html).toContain('Dining merchants')
  expect(html).not.toContain('Travel merchants')
  expect(html).toContain('data-dimmed="true"')
})

it('marks the compared amount on the track and names it for assistive technology', () => {
  const html = renderToStaticMarkup(
    <RankedBars items={items} label="Spending by category" emptyMessage="None." />,
  )
  expect(html.match(/class="ranked-bars-mark"/g)).toHaveLength(1)
  expect(html).toContain(', usually by this point $25.00')
  expect(html).not.toContain('aria-pressed')
})

it('draws no chevron on a selectable row with nothing to expand', () => {
  const html = renderToStaticMarkup(
    <RankedBars
      items={[
        { name: 'Dining', value: -40, color: 'red' },
        { name: 'Travel', value: -30, color: 'blue' },
      ]}
      label="Spending by category"
      emptyMessage="None."
      onSelect={() => undefined}
    />,
  )
  expect(html.match(/aria-pressed="false"/g)).toHaveLength(2)
  expect(html).not.toContain('ranked-bars-chevron')
})
