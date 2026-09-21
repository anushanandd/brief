import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { HomeSummaryText } from './home-summary'

it('renders two bullets with evidence-backed payment and income colors even for equal amounts', () => {
  const payment = 'Estimated payment of $12.00 for Example Membership in 4 days (9/24).'
  const income = 'Estimated income of $12.00 for Example Interest in 4 days (9/24).'
  const evidence = JSON.stringify({
    upcoming: [
      { sentence: payment, direction: 'outflow', amount: '$12.00' },
      { sentence: income, direction: 'inflow', amount: '$12.00' },
    ],
  })
  const html = renderToStaticMarkup(
    <HomeSummaryText sentences={[payment, income]} evidence={evidence} />,
  )
  expect(html.match(/<li>/g)).toHaveLength(2)
  expect(html).toContain('<strong class="negative">$12.00</strong> for Example Membership')
  expect(html).toContain('<strong class="positive">$12.00</strong> for Example Interest')
  expect(html).toContain('<strong>in 4 days (9/24)</strong>')
})

it('leaves an amount neutral when the summary cannot be associated with its source event', () => {
  const evidence = JSON.stringify({
    upcoming: [
      {
        sentence: 'Estimated income of $12.00 for Example Interest today (9/20).',
        direction: 'inflow',
        amount: '$12.00',
      },
    ],
  })
  const html = renderToStaticMarkup(
    <HomeSummaryText
      sentences={['Spending was $12.00.', 'Saved activity is incomplete.']}
      evidence={evidence}
    />,
  )
  expect(html).toContain('<strong>$12.00</strong>')
})
