import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it, vi } from 'vitest'

import { CommandMenu } from './command-menu'

vi.mock('@tanstack/react-router', () => ({ useNavigate: () => vi.fn() }))
vi.mock('../hooks/use-finance', () => ({ useFinance: () => ({ data: { accounts: [] } }) }))
vi.mock('../hooks/use-refresh-finance', () => ({ useRefreshFinance: () => vi.fn() }))

it('renders an accessible combobox linked to its list of commands', () => {
  const html = renderToStaticMarkup(<CommandMenu open onOpenChange={vi.fn()} />)
  const listId = html.match(/aria-controls="([^"]+)"/)?.[1]
  expect(listId).toBeTruthy()
  expect(html).toContain('role="combobox"')
  expect(html).toContain('aria-expanded="true"')
  expect(html).toMatch(new RegExp(`role="listbox"[^>]*id="${listId}"`))
  expect(html).toContain('role="option"')
  expect(html).toContain('Refresh snapshot')
  expect(html).toContain('Review data sources')
})
