// @vitest-environment jsdom
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { BenefitMark, FeeRecovery } from './card-benefits'

const mark = (name: string, logoDomain: string | undefined, externalLogosEnabled: boolean) =>
  renderToStaticMarkup(
    <BenefitMark
      id={name.toLowerCase()}
      name={name}
      logoDomain={logoDomain}
      externalLogosEnabled={externalLogosEnabled}
    />,
  )

it('marks a benefit with its logo, else an icon for its kind, else initials', () => {
  expect(mark('Example Perk', 'perk.example.com', true)).toContain('img.logo.dev/perk.example.com')
  const offline = mark('Example Perk', 'perk.example.com', false)
  expect(offline).not.toContain('logo.dev')
  expect(offline).toContain('Example Perk logo')
  expect(mark('Dining', undefined, true)).toContain('benefit-mark-dining')
  expect(mark('Dining', 'tables.example.com', false)).toContain('benefit-mark-dining')
})

const markOf = (fields: Partial<React.ComponentProps<typeof BenefitMark>>) =>
  renderToStaticMarkup(
    <BenefitMark id="perk" name="Example Perk" externalLogosEnabled={false} {...fields} />,
  )

it('tries the domain, then the purchase logo Plaid attached, then the brand, then icons', () => {
  const plaid = 'https://plaid-merchant-logos.plaid.com/perk.png'
  expect(
    markOf({ logoUrl: plaid, logoDomain: 'perk.example.com', externalLogosEnabled: true }),
  ).toContain('img.logo.dev/perk.example.com')
  expect(
    markOf({ logoUrl: plaid, logoBrand: 'examplebrand', externalLogosEnabled: true }),
  ).toContain(plaid)
  expect(markOf({ logoBrand: 'example brand', externalLogosEnabled: true })).toContain(
    'img.logo.dev/name/example%20brand',
  )
  expect(markOf({ logoBrand: 'examplebrand', category: 'dining' })).toContain('benefit-mark-dining')
  // Only Plaid's own logo host loads, and nothing external with logos off.
  expect(
    markOf({ logoUrl: 'https://elsewhere.example/perk.png', externalLogosEnabled: true }),
  ).not.toContain('elsewhere')
  expect(markOf({ logoUrl: plaid })).not.toContain('plaid-merchant-logos')
  expect(markOf({ category: 'rideshare' })).toContain('benefit-mark-rides')
  expect(markOf({ category: 'dining', name: 'Example Stream' })).toContain('benefit-mark-dining')
  expect(markOf({ name: 'Example Ride credit' })).toContain('benefit-mark-rides')
  expect(markOf({ name: 'Security screening' })).toContain('benefit-mark-security')
  expect(markOf({ name: 'Example Perk' })).toContain('Example Perk logo')
})

it('moves to the next logo when one fails to load, and to an icon after the last', async () => {
  const container = document.createElement('div')
  document.body.append(container)
  const root = createRoot(container)
  await act(async () =>
    root.render(
      <BenefitMark
        id="perk"
        name="Example Perk"
        logoDomain="perk.example.com"
        logoUrl="https://plaid-merchant-logos.plaid.com/perk.png"
        logoBrand="examplebrand"
        category="dining"
        externalLogosEnabled
      />,
    ),
  )
  const sources: string[] = []
  for (let image = container.querySelector('img'); image; image = container.querySelector('img')) {
    sources.push(new URL(image.src).host + new URL(image.src).pathname)
    await act(async () => image?.dispatchEvent(new Event('error')))
  }
  expect(sources).toEqual([
    'img.logo.dev/perk.example.com',
    'plaid-merchant-logos.plaid.com/perk.png',
    'img.logo.dev/name/examplebrand',
  ])
  expect(container.querySelector('.benefit-mark-dining')).not.toBeNull()
  await act(async () => root.unmount())
  container.remove()
})

const fee = {
  annualFee: 450,
  recovered: 239.69,
  offsets: 0,
  covered: false,
  gap: 210.31,
  forOthers: 92.99,
  since: '2026-05-09',
  basis: 'card-history' as const,
  through: '2026-12-31',
  expected: 120,
}

it('gives the fee year and its estimate under the meter, in whole dollars', () => {
  const html = renderToStaticMarkup(<FeeRecovery fee={fee} />)
  expect(html).toContain('$240</strong> of the <span class="sensitive-value">$450</span> fee back')
  expect(html).toContain(' · $210 to go · $93 for others')
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
    <FeeRecovery fee={{ ...fee, offsets: 450, covered: true, gap: 239.69 }} />,
  )
  expect(repaid).toContain(' · $450 repaid by an offset')
  expect(repaid).toContain('$240 past break-even')
})
