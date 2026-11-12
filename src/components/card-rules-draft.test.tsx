import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { benefitDraft, cardDraft, termCheck } from '../data/fixtures/card-drafts'
import { ChangeList, ConfidenceChip, DraftTable, UnmatchedCredits } from './card-rules-draft'

const drafted = cardDraft([
  benefitDraft('streaming', [
    termCheck('cap', 'explained', { sentence: 0, source: 'terms' }),
    termCheck('alignment', 'suggested'),
    termCheck('creditMatches', 'observed', {
      source: 'history',
      quote: 'TEST BANK STREAMING CREDIT',
    }),
  ]),
  benefitDraft('dining', [termCheck('cap', 'conflict', { note: 'The terms say $25' })], false),
])

it('labels each drafted term by its evidence and cites the sentence or credit behind it', () => {
  const html = renderToStaticMarkup(<DraftTable draft={drafted} edits={{}} onEdit={() => {}} />)
  for (const label of ['Explained', 'Suggested', 'Observed', 'Conflict']) {
    expect(html).toContain(`>${label}</span>`)
  }
  expect(html).toContain(
    '<blockquote>You get up to $10 each month in streaming credits.</blockquote>',
  )
  expect(html).toContain('<blockquote>TEST BANK STREAMING CREDIT</blockquote>')
  expect(html).toContain('The terms say $25')
  // A benefit the draft leaves out reads as left out, unchecked.
  expect(html).toMatch(/<tr[^>]*class="is-left-out"/)
  expect(html).toMatch(/aria-label="Keep streaming" checked=""/)
})

it('shows the user’s edits in place of the drafted terms', () => {
  const html = renderToStaticMarkup(
    <DraftTable
      draft={drafted}
      edits={{ dining: { include: true, cap: 30, creditMatches: 'dining credit' } }}
      onEdit={() => {}}
    />,
  )
  expect(html).not.toContain('is-left-out')
  expect(html).toContain('value="30"')
  expect(html).toContain('value="dining credit"')
})

it('offers each unexplained credit to the benefits the draft keeps', () => {
  const html = renderToStaticMarkup(
    <UnmatchedCredits draft={drafted} edits={{}} onEdit={() => {}} />,
  )
  expect(html).toContain('<code>TEST BANK STREAMING CREDIT</code>')
  expect(html).toContain('Add TEST BANK STREAMING CREDIT to a benefit')
})

it('lists a change’s terms before and after', () => {
  const html = renderToStaticMarkup(
    <ChangeList
      change={{
        product: 'test-rewards',
        name: 'Test Rewards Card',
        reason: 'The catalog entry changed',
        draft: drafted,
        changes: [{ benefit: 'Streaming', field: 'cap', before: '$10', after: '$15' }],
      }}
    />,
  )
  expect(html).toContain('<del>$10</del>')
  expect(html).toContain('<ins>$15</ins>')
})

it('never relies on color alone for confidence', () => {
  expect(renderToStaticMarkup(<ConfidenceChip confidence="unconfirmed" />)).toContain(
    'title="Nothing supports it yet">Unconfirmed<',
  )
})
