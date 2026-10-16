import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { BriefLine } from './brief-line'

it('renders Rust’s spans as written, masking personal amounts', () => {
  const html = renderToStaticMarkup(
    <BriefLine
      spans={[
        { kind: 'text', text: 'Your investments fell ' },
        { kind: 'figure', text: '0.9%', tone: 'down', sensitive: false },
        { kind: 'text', text: ' (' },
        { kind: 'figure', text: '−$1,240', tone: 'down', sensitive: true },
        { kind: 'text', text: ') today. ' },
        { kind: 'subject', text: 'NVDA', ticker: 'NVDA' },
      ]}
    />,
  )
  expect(html).toBe(
    'Your investments fell <strong class="negative">0.9%</strong> (<strong class="negative sensitive-value">−$1,240</strong>) today. <strong>NVDA</strong>',
  )
})
