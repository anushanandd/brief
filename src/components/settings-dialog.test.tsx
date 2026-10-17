// @vitest-environment jsdom

import { expect, it } from 'vitest'

import { escapeClosesSettings } from './settings-dialog'

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
