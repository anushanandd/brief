import { Save } from 'lucide-react'
import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { selectedFilterValues, toggleFilterValue } from './ledger-filters'
import { AnimatedCurrency, Button, Change, RangeSelector } from './ui'

it('renders available currency with NumberFlow and keeps unavailable values static', () => {
  expect(
    renderToStaticMarkup(<AnimatedCurrency className="hero-number" value={1234.5} />),
  ).toContain('<number-flow-react')
  expect(renderToStaticMarkup(<AnimatedCurrency value={null} />)).toContain('—')
})

it('uses context when coloring changes', () => {
  expect(renderToStaticMarkup(<Change value={10} />)).toContain('change positive')
  expect(renderToStaticMarkup(<Change value={10} favorable="decrease" />)).toContain(
    'change negative',
  )
})

it.each([0, -0, null, undefined])('uses a neutral tone for %s', (value) => {
  expect(renderToStaticMarkup(<Change value={value} />)).toContain('change muted')
})

it('parses and toggles multi-select ledger filters', () => {
  expect(selectedFilterValues('cash,missing,credit', ['cash', 'credit'])).toEqual([
    'cash',
    'credit',
  ])
  expect(toggleFilterValue(['cash'], 'credit')).toEqual(['cash', 'credit'])
  expect(toggleFilterValue(['cash', 'credit'], 'cash')).toEqual(['credit'])
  expect(toggleFilterValue(['cash'], '')).toEqual(['cash'])
})

it('labels shared range controls with their full meanings', () => {
  const html = renderToStaticMarkup(
    <RangeSelector
      label="Chart range"
      options={[
        { value: 'week', label: 'W', accessibleLabel: 'One week' },
        { value: 'month', label: 'M', accessibleLabel: 'One month' },
      ]}
      value="week"
      onValueChange={() => undefined}
    />,
  )

  expect(html).toContain('role="group" aria-label="Chart range"')
  expect(html).toMatch(/aria-label="One week"[^>]*aria-pressed="true"/)
  expect(html).toMatch(/aria-label="One month"[^>]*aria-pressed="false"/)
  expect(html).not.toContain('title=')
  expect(html).not.toContain('data-slot="tooltip-trigger"')
})

it('keeps icon action labels, disabled state, and native form semantics', () => {
  const html = renderToStaticMarkup(
    <Button icon={Save} type="submit" disabled aria-busy>
      Save changes
    </Button>,
  )
  expect(html).toContain('type="submit"')
  expect(html).toContain('disabled=""')
  expect(html).toContain('aria-busy="true"')
  expect(html).toContain('<span class="sr-only">Save changes</span>')
  expect(html).toMatch(/<svg[^>]*aria-hidden="true"/)
})
