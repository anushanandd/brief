// @vitest-environment jsdom

import { act, memo, useState } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it } from 'vitest'

import {
  announceAmountsHidden,
  formatCurrency,
  HIDDEN_AMOUNT,
  setAmountsHidden,
} from '../lib/format'
import { maskedPage, useAmountsHidden } from './use-amounts-hidden'

/** A page holding an unsaved draft, as a transaction's note and split drafts are held. */
function DraftPage() {
  const [draft, setDraft] = useState('')
  return (
    <>
      <output>{formatCurrency(-42.5)}</output>
      <button type="button" onClick={() => setDraft('Dinner with Example Person A')}>
        Type
      </button>
      <textarea value={draft} readOnly />
    </>
  )
}
const Page = maskedPage(DraftPage)
// The router memoizes its matches, so the shell rendering again never reaches the page.
const Match = memo(function Match() {
  return <Page />
})

/** A memoized list caching a formatted amount, keyed on the mask. */
const CachedList = memo(function CachedList() {
  const hidden = useAmountsHidden()
  const [cached] = useState(() => ({ hidden, text: formatCurrency(12) }))
  return <p>{cached.hidden === hidden ? cached.text : formatCurrency(12)}</p>
})

let root: Root

beforeEach(() => {
  setAmountsHidden(false)
  root = createRoot(document.body.appendChild(document.createElement('div')))
})

afterEach(async () => {
  await act(async () => root.unmount())
  setAmountsHidden(false)
  document.body.replaceChildren()
})

/** What the shell does when ⌘H or hiding on blur changes the mask. */
function hide(hidden: boolean) {
  act(() => {
    setAmountsHidden(hidden)
    announceAmountsHidden()
  })
}

it('masks a page in place, keeping its unsaved draft through hiding and showing', async () => {
  await act(async () =>
    root.render(
      <>
        <Match />
        <CachedList />
      </>,
    ),
  )
  const textarea = document.querySelector('textarea')!
  act(() => document.querySelector('button')?.click())
  expect(document.querySelector('output')?.textContent).toBe('−$42.50')

  hide(true)
  expect(document.querySelector('output')?.textContent).toBe(HIDDEN_AMOUNT)
  expect(document.querySelector('p')?.textContent).toBe(HIDDEN_AMOUNT)
  // The same element, with what was typed: nothing remounted.
  expect(document.querySelector('textarea')).toBe(textarea)
  expect(textarea.value).toBe('Dinner with Example Person A')

  hide(false)
  expect(document.querySelector('output')?.textContent).toBe('−$42.50')
  expect(document.querySelector('p')?.textContent).toBe('$12.00')
  expect(textarea.value).toBe('Dinner with Example Person A')
})
