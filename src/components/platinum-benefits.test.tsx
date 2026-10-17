import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { FeeRecovery } from './platinum-benefits'

const fee = {
  annualFee: 895,
  recovered: 539.69,
  offsets: 0,
  covered: false,
  gap: 355.31,
  forOthers: 492.99,
  since: '2026-05-09',
  basis: 'card-history' as const,
  through: '2026-12-31',
  expected: 120,
}

it('gives the fee year and its estimate under the meter, in whole dollars', () => {
  const html = renderToStaticMarkup(<FeeRecovery fee={fee} />)
  expect(html).toContain('$540</strong> of the <span class="sensitive-value">$895</span> fee back')
  expect(html).toContain(' · $355 to go · $493 for others')
  // The feed has no anniversary, so a year without a fee charge says so.
  expect(html).toContain('May 9, 2026 – Dec 31, 2026 · no fee charge saved')
  expect(renderToStaticMarkup(<FeeRecovery fee={{ ...fee, basis: 'fee-charge' }} />)).not.toContain(
    'no fee charge saved',
  )
  // Only what the periods already plan.
  expect(html).toContain('$120</span> more estimated')
  expect(renderToStaticMarkup(<FeeRecovery fee={{ ...fee, expected: 0 }} />)).not.toContain(
    'more estimated',
  )
  // A bonus filed as repaying the fee is named, and Rust says when the two cover the fee.
  const repaid = renderToStaticMarkup(
    <FeeRecovery fee={{ ...fee, offsets: 895, covered: true, gap: 539.69 }} />,
  )
  expect(repaid).toContain(' · $895 repaid by an offset')
  expect(repaid).toContain('$540 past break-even')
})
