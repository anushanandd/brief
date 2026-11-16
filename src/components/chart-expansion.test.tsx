// @vitest-environment jsdom
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it } from 'vitest'

import { ActionProvider } from '../lib/action-system'
import { ExpandableChart } from './charts'

let root: Root

beforeEach(() => {
  // jsdom has no top layer; opening and closing set the attribute.
  HTMLDialogElement.prototype.showModal = function () {
    this.setAttribute('open', '')
  }
  HTMLDialogElement.prototype.close = function () {
    this.removeAttribute('open')
  }
  root = createRoot(document.body.appendChild(document.createElement('div')))
})

afterEach(async () => {
  await act(async () => root.unmount())
  document.body.replaceChildren()
})

it('names the expanded window without repeating the name its content already shows', async () => {
  await act(async () =>
    root.render(
      <ActionProvider>
        <ExpandableChart title="Savings">
          {(expanded, expand) => (
            <>
              <h2>Savings</h2>
              {expanded ? null : (
                <button type="button" onClick={expand}>
                  Expand
                </button>
              )}
            </>
          )}
        </ExpandableChart>
      </ActionProvider>,
    ),
  )
  await act(async () => document.querySelector('button')?.click())
  const dialog = document.querySelector('dialog')!
  expect(dialog.open).toBe(true)
  const label = document.getElementById(dialog.getAttribute('aria-labelledby') ?? '')
  expect(label?.textContent).toBe('Savings')
  expect(document.activeElement).toBe(label)
  const visible = [...dialog.querySelectorAll('h2')].filter(
    (heading) => !heading.classList.contains('sr-only'),
  )
  expect(visible.map((heading) => heading.textContent)).toEqual(['Savings'])
  // Escape and the backdrop close it, so it has no close button of its own.
  expect(dialog.querySelector('button, header')).toBeNull()

  // The cancel event is what Escape sends a modal dialog.
  await act(async () => dialog.dispatchEvent(new Event('cancel', { cancelable: true })))
  expect(dialog.open).toBe(false)
  expect(dialog.childElementCount).toBe(0)
})

it('closes on a click on the backdrop', async () => {
  await act(async () =>
    root.render(
      <ActionProvider>
        <ExpandableChart title="Savings">
          {(expanded, expand) =>
            expanded ? null : (
              <button type="button" onClick={expand}>
                Expand
              </button>
            )
          }
        </ExpandableChart>
      </ActionProvider>,
    ),
  )
  await act(async () => document.querySelector('button')?.click())
  const dialog = document.querySelector('dialog')!
  await act(async () => {
    dialog.dispatchEvent(new Event('pointerdown', { bubbles: true }))
    dialog.click()
  })
  expect(dialog.open).toBe(false)
})
