import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { seedSummary } from '../data/fixtures/records'
import { AsOf, Headlines } from './investment-summary-parts'

// The browser preview's summary, made by Rust from the synthetic seed.
const summary = seedSummary()

it('says when the prices are from', () => {
  expect(renderToStaticMarkup(<AsOf summary={summary} />)).toContain(
    `As of <time dateTime="${summary.pricesAsOf}">`,
  )
  expect(renderToStaticMarkup(<AsOf summary={{ ...summary, pricesAsOf: null }} />)).toBe('')
})

it('lists each headline as published, linked, with its publisher and time', () => {
  const html = renderToStaticMarkup(<Headlines summary={summary} />)
  const [first] = summary.brief!.headlines
  expect(html).toContain(`href="${first.url}"`)
  expect(html).toContain(`<span>${first.title}</span>`)
  expect(html).toContain(`<small>CNBC · <time dateTime="${first.publishedAt}">`)
  expect(
    renderToStaticMarkup(
      <Headlines summary={{ ...summary, brief: { ...summary.brief!, headlines: [] } }} />,
    ),
  ).toBe('')
})
