import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { RefreshProgress } from './refresh-progress'

it('keeps refresh progress indeterminate and explains snapshot safety', () => {
  const markup = renderToStaticMarkup(
    <RefreshProgress
      detail="Connected sources update independently."
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
