import { renderToString } from 'react-dom/server'
import { expect, it } from 'vitest'

import { searchSettings } from '../lib/settings-search'
import { SettingsCard, SettingsRow, SettingsSearchProvider, SettingsSection } from './settings-ui'

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
      <SettingsSection id="appearance" title="Appearance">
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
  expect(hidden(html, 'appearance')).toBe(true)
  expect(html).toContain('<mark>backup</mark>')
  expect(html).not.toContain('<button>Reset</button>')
})
