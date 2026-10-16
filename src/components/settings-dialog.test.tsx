// @vitest-environment jsdom

import { expect, it } from 'vitest'

import { backdropClick, escapeClosesSettings } from './settings-dialog'

it('closes Settings on Escape from fields and controls but not from an open menu', () => {
  document.body.innerHTML = `
    <input type="search" id="search" value="theme" />
    <button id="trigger" aria-expanded="true">Week</button>
    <div role="listbox"><div role="option" id="option">Month</div></div>
    <button id="plain">Reset</button>`
  expect(escapeClosesSettings(document.getElementById('search'))).toBe(true)
  expect(escapeClosesSettings(document.getElementById('plain'))).toBe(true)
  expect(escapeClosesSettings(null)).toBe(true)
  expect(escapeClosesSettings(document.getElementById('trigger'))).toBe(false)
  expect(escapeClosesSettings(document.getElementById('option'))).toBe(false)
})

it('closes Settings only for a press that starts and ends on the backdrop', () => {
  document.body.innerHTML = `
    <dialog id="dialog"><div role="listbox"><div role="option" id="option">Expanded</div></div></dialog>`
  const dialog = document.getElementById('dialog')!
  const option = document.getElementById('option')!
  expect(backdropClick(dialog, dialog, dialog)).toBe(true)
  // Choosing an option unmounts the menu, so the click retargets to the dialog.
  expect(backdropClick(dialog, option, dialog)).toBe(false)
  expect(backdropClick(dialog, dialog, option)).toBe(false)
  // A keyboard-activated click has no press.
  expect(backdropClick(dialog, null, dialog)).toBe(false)
})
