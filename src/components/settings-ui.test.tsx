// @vitest-environment jsdom

import { act, useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { renderToString } from 'react-dom/server'
import { expect, it } from 'vitest'

import { searchSettings } from '../lib/settings-search'
import {
  InlineConfirm,
  SettingsCard,
  SettingsRow,
  SettingsSearchProvider,
  SettingsSection,
} from './settings-ui'

const sections = (query: string) =>
  renderToString(
    <SettingsSearchProvider value={searchSettings(query)}>
      <SettingsSection id="data" title="Backup & data" action={<button>Reset</button>}>
        <SettingsCard>
          <SettingsRow id="financial-backup" title="Financial backup">
            <button>Save</button>
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>
      <SettingsSection id="navigation" title="Navigation">
        <SettingsCard>
          <SettingsRow id="theme" title="Theme">
            <button>Light</button>
          </SettingsRow>
        </SettingsCard>
      </SettingsSection>
    </SettingsSearchProvider>,
  )

const hidden = (html: string, id: string) => new RegExp(`<[^>]*id="${id}"[^>]*hidden=""`).test(html)

it('shows every section, row, and reset action without a search', () => {
  const html = sections('')
  expect(hidden(html, 'data')).toBe(false)
  expect(hidden(html, 'theme')).toBe(false)
  expect(html).toContain('<button>Reset</button>')
  expect(html).not.toContain('<mark>')
})

it('hides non-matching rows and sections and highlights matched title text', () => {
  const html = sections('backup')
  expect(hidden(html, 'financial-backup')).toBe(false)
  expect(hidden(html, 'data')).toBe(false)
  expect(hidden(html, 'theme')).toBe(true)
  expect(hidden(html, 'navigation')).toBe(true)
  expect(html).toContain('<mark>backup</mark>')
  expect(html).not.toContain('<button>Reset</button>')
})

function Erase() {
  const [confirming, setConfirming] = useState(false)
  const trigger = useRef<HTMLButtonElement>(null)
  return confirming ? (
    <InlineConfirm
      label="Erase"
      onCancel={() => setConfirming(false)}
      returnFocus={() => trigger.current}
      actions={<button>Erase now</button>}
    >
      <p>Sure?</p>
    </InlineConfirm>
  ) : (
    <button ref={trigger} onClick={() => setConfirming(true)}>
      Erase…
    </button>
  )
}

it('returns focus to a confirmation’s trigger even when the trigger was replaced', async () => {
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () => root.render(<Erase />))
  await act(async () => document.querySelector('button')?.click())
  expect(document.activeElement?.textContent).toBe('Cancel')
  await act(async () =>
    document.activeElement?.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true }),
    ),
  )
  expect(document.activeElement?.textContent).toBe('Erase…')
  await act(async () => root.unmount())
  document.body.replaceChildren()
})
