import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { Save } from './icons'
import { selectedFilterValues } from './ledger-filters'
import {
  AnimatedCurrency,
  Button,
  Change,
  ChartRangeSelect,
  hasMoreBelow,
  RangeSelector,
} from './ui'

it('shows a card scroll cue only while content remains below', () => {
  expect(hasMoreBelow({ scrollHeight: 300, clientHeight: 200, scrollTop: 0 })).toBe(true)
  expect(hasMoreBelow({ scrollHeight: 300, clientHeight: 200, scrollTop: 100 })).toBe(false)
  expect(hasMoreBelow({ scrollHeight: 200, clientHeight: 200, scrollTop: 0 })).toBe(false)
})

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

it('parses available ledger filter values', () => {
  expect(selectedFilterValues('cash,missing,credit', ['cash', 'credit'])).toEqual([
    'cash',
    'credit',
  ])
})

it('renders graph ranges as an accessible dropdown', () => {
  const html = renderToStaticMarkup(
    <ChartRangeSelect
      label="Chart range"
      options={[
        { value: 'week', label: 'W', accessibleLabel: 'One week' },
        { value: 'month', label: 'M', accessibleLabel: 'One month' },
      ]}
      value="week"
      onValueChange={() => undefined}
    />,
  )

  expect(html).toMatch(/role="combobox"[^>]*aria-label="Chart range"/)
  expect(html).toContain('ledger-select-trigger chart-range-select')
  expect(html).toContain('One week')
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
