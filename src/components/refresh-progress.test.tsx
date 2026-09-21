import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import { RefreshProgress } from './refresh-progress'

afterEach(() => vi.useRealTimers())

it('shows provider progress without inventing completion percentages', () => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(10_000)
  const markup = renderToStaticMarkup(
    <RefreshProgress
      startedAt={0}
      tasks={[
        { id: 'plaid', label: 'Plaid', status: 'Complete', state: 'complete' },
        { id: 'snaptrade', label: 'SnapTrade', status: 'Checking', state: 'active' },
      ]}
    />,
  )

  expect(markup).toContain('Plaid')
  expect(markup).toContain('SnapTrade')
  expect(markup).toContain('Complete')
  expect(markup).toContain('Checking')
  expect(markup).toContain('role="progressbar"')
  expect(markup).not.toContain('aria-valuenow')
  expect(markup).toContain('Your saved data remains available')

  const finished = renderToStaticMarkup(
    <RefreshProgress detail="Validated and saved locally." startedAt={Date.now()} finished />,
  )
  expect(finished).not.toContain('role="progressbar"')
  expect(finished).toContain('0s total')
})
