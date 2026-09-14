import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { DonutChart } from './charts'

it('lists the five largest donut values without connector lines', () => {
  const markup = renderToStaticMarkup(
    <DonutChart
      segments={[
        { name: 'Travel', value: 50, color: '#fff' },
        { name: 'Dining', value: 10, color: '#aaa' },
        { name: 'Shopping', value: 10, color: '#999' },
        { name: 'Utilities', value: 10, color: '#888' },
        { name: 'Groceries', value: 10, color: '#777' },
        { name: 'Entertainment', value: 10, color: '#666' },
        { name: 'Empty', value: 0, color: '#000' },
      ]}
      label="Spending by category"
      centerValue="$100"
      centerLabel="spent"
    />,
  )

  expect(markup).toContain('donut-chart-layout')
  expect(markup.match(/class="donut-legend-item"/g)).toHaveLength(5)
  expect(markup).not.toContain('<polyline')
  expect(markup.indexOf('>Travel<')).toBeLessThan(markup.indexOf('>Dining<'))
  expect(markup).not.toContain('>Entertainment<')
  expect(markup).not.toContain('>Empty<')
})
