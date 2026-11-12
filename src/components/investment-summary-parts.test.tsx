import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import nativeSummary from '../data/fixtures/native-summary.json'
import { seedSummary } from '../data/fixtures/records'
import { fromRust } from '../lib/rust-json'
import type { InvestmentSummary } from '../lib/schema'
import { AsOf, Changed, Headlines, MarketWrap } from './investment-summary-parts'

// A summary as Rust serializes it; a Rust test keeps this fixture in sync.
const summary = fromRust<InvestmentSummary>(nativeSummary)
// The browser preview's summary, made by Rust from the synthetic seed.
const preview = seedSummary()

it('says when the prices are from', () => {
  expect(renderToStaticMarkup(<AsOf summary={preview} />)).toContain(
    `As of <time dateTime="${preview.pricesAsOf}">`,
  )
  expect(renderToStaticMarkup(<AsOf summary={{ ...preview, pricesAsOf: null }} />)).toBe('')
})

it('lists each headline as published, linked, with its publisher and time', () => {
  const html = renderToStaticMarkup(<Headlines summary={preview} />)
  const [first] = preview.brief!.headlines
  if (!first) throw new Error('The preview has headlines')
  expect(html).toContain(`href="${first.url}"`)
  expect(html).toContain(`<span>${first.title}</span>`)
  expect(html).toContain(`<small>CNBC · <time dateTime="${first.publishedAt}">`)
  expect(
    renderToStaticMarkup(
      <Headlines summary={{ ...preview, brief: { ...preview.brief!, headlines: [] } }} />,
    ),
  ).toBe('')
})

it('leads with a sentence a reader picked, quoted, and keeps a summary under its headline', () => {
  const brief = summary.brief!
  const [story] = brief.headlines
  if (!story) throw new Error('The summary has headlines')
  const picked: InvestmentSummary = {
    ...summary,
    brief: {
      ...brief,
      headlines: [
        {
          ...story,
          title: 'Why Is Test Corp Stock Rising Today?',
          quote: 'The stock rose after the company signed a supply deal.',
          quotedBy: 'apple',
        },
      ],
    },
  }
  const html = renderToStaticMarkup(<Headlines summary={picked} />)
  expect(html).toContain('“The stock rose after the company signed a supply deal.”')
  expect(html).toContain('title="Sentence picked from the story by Apple Intelligence"')
  // The explainer's own headline says nothing, so it isn't shown.
  expect(html).not.toContain('Why Is Test Corp Stock Rising Today?')
  // A publisher's summary sits under the headline it summarizes, unquoted.
  const plain = renderToStaticMarkup(<Headlines summary={summary} />)
  expect(plain).toContain('Test Corp jumps after naming a new finance chief')
  expect(plain).toContain(
    '<span class="brief-story-detail">The shares rose after the company named a new chief financial officer.</span>',
  )
})

it('shows the market’s wrap and what changed only when Rust has them', () => {
  expect(renderToStaticMarkup(<MarketWrap summary={summary} />)).toBe('')
  expect(renderToStaticMarkup(<Changed summary={summary} />)).toBe('')
  const [event] = summary.events
  if (!event) throw new Error('The summary has events')
  const full: InvestmentSummary = {
    ...summary,
    brief: {
      ...summary.brief!,
      market: {
        tickers: [],
        title: 'Stocks rise as tech rallies',
        publisher: 'CNBC',
        url: 'https://example.com/wrap',
        publishedAt: '2026-09-30T20:30:00Z',
        quote: 'Technology shares led a broad advance.',
        quotedBy: 'summary',
      },
      changed: [event],
    },
  }
  const market = renderToStaticMarkup(<MarketWrap summary={full} />)
  expect(market).toContain('<h3>Market</h3>')
  expect(market).toContain('Stocks rise as tech rallies')
  expect(market).toContain('href="https://example.com/wrap"')
  const changed = renderToStaticMarkup(<Changed summary={full} />)
  expect(changed).toContain('<h3>Changed</h3>')
  expect(changed).toContain(event.title.replace(/&/g, '&amp;'))
})

it('tells an earnings report in Rust’s words and opens its release', () => {
  const [digest] = summary.earnings
  if (!digest) throw new Error('The summary has a report')
  const html = renderToStaticMarkup(
    <Changed summary={{ ...summary, brief: { ...summary.brief!, earnings: [digest] } }} />,
  )
  // Every word of the line is Rust's; figures are styled, not reworded.
  const text = html.replace(/<[^>]+>/g, '')
  expect(text).toContain(digest.line.map((span) => span.text).join(''))
  expect(html).toContain(`href="${digest.releaseUrl}"`)
  expect(html).toContain('gave guidance')
})
