import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { Change } from './ui'

it('uses context when coloring changes', () => {
  expect(renderToStaticMarkup(<Change value={10} />)).toContain('change positive')
  expect(renderToStaticMarkup(<Change value={10} favorable="decrease" />)).toContain(
    'change negative',
  )
})
