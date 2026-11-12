// @vitest-environment jsdom

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { getDataHealth } from '../lib/api'
import { ConnectionNotice } from './all-accounts-overview'

vi.mock('@tanstack/react-router', () => ({ Link: () => null, useNavigate: () => vi.fn() }))
vi.mock('../lib/api', () => ({ getDataHealth: vi.fn() }))

let root: Root
let client: QueryClient

beforeEach(() => {
  client = new QueryClient()
  root = createRoot(document.body.appendChild(document.createElement('div')))
})

afterEach(async () => {
  await act(async () => root.unmount())
  client.clear()
  document.body.replaceChildren()
})

async function render() {
  await act(async () =>
    root.render(
      <QueryClientProvider client={client}>
        <ConnectionNotice snapshot={{ revision: 1, updatedAt: '2026-01-01T00:00:00Z' }} />
      </QueryClientProvider>,
    ),
  )
}

const notice = () => document.querySelector('[role="status"]')?.textContent ?? ''

it("says when Data health couldn't be checked, rather than reading as fine", async () => {
  vi.mocked(getDataHealth).mockRejectedValue(new Error('Synthetic health failure'))
  await render()
  await act(async () => {
    await vi.waitFor(() => expect(notice()).toContain('Couldn’t check connections'))
  })
  expect(notice()).toContain('Synthetic health failure')

  vi.mocked(getDataHealth).mockResolvedValue({
    computedAt: '2026-09-10T12:00:00Z',
    revision: 1,
    overallStatus: 'ok',
    counts: { critical: 0, error: 0, warning: 0, info: 0 },
    issues: [],
  })
  const retry = [...document.querySelectorAll('button')].find(
    ({ textContent }) => textContent === 'Try again',
  )
  await act(async () => retry?.click())
  await act(async () => {
    await vi.waitFor(() => expect(document.querySelector('[role="status"]')).toBeNull())
  })
})
