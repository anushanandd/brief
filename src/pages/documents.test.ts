// @vitest-environment jsdom

import { afterEach, expect, it } from 'vitest'

import { spaceActivates } from '../hooks/use-keyboard-dispatch'

afterEach(() => document.body.replaceChildren())

it("leaves Space to the viewer's buttons and fields, and closes the viewer otherwise", () => {
  const page = document.body.appendChild(document.createElement('button'))
  const viewer = document.body.appendChild(document.createElement('div'))
  viewer.innerHTML =
    '<button>Open in Preview</button><a href="#">Link</a><textarea></textarea>' +
    '<select></select><div contenteditable="true"></div><div class="body"></div>'
  for (const selector of ['button', 'a', 'textarea', 'select', '[contenteditable]'])
    expect(spaceActivates(viewer.querySelector(selector), viewer)).toBe(true)
  expect(spaceActivates(viewer.querySelector('.body'), viewer)).toBe(false)
  // Focus left on the page behind the viewer doesn't keep Space from closing it.
  expect(spaceActivates(page, viewer)).toBe(false)
  expect(spaceActivates(null, viewer)).toBe(false)
})
