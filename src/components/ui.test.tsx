// @vitest-environment jsdom
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, expect, it } from 'vitest'

import { setAmountsHidden } from '../lib/format'
import { Save } from './icons'
import {
  AnimatedCurrency,
  Button,
  Change,
  ChartChange,
  ChartRangeSelect,
  hasMoreBelow,
} from './ui'

afterEach(() => setAmountsHidden(false))

const parse = (html: string) => {
  const host = document.createElement('div')
  host.innerHTML = html
  return host
}
// NumberFlow renders its digits into a shadow root, which static markup keeps in templates.
const flows = (html: string) =>
  [...parse(html).querySelectorAll('template')].map(({ content }) => content)
const flowText = (html: string) =>
  flows(html)
    .map((root) => root.querySelector('[role="img"]')?.getAttribute('aria-label'))
    .join('')
const styledParts = (root: ParentNode, selector: (part: string) => string) =>
  ['currency', 'integer', 'fraction'].map(
    (part) => root.querySelector(selector(part))?.textContent ?? null,
  )

it.each([1234.5, -1234.5, 0, 1234567.89])(
  'writes a plain figure as NumberFlow does, in the same parts, for %s',
  (value) => {
    const flow = renderToStaticMarkup(<AnimatedCurrency className="hero-number" value={value} />)
    const plain = renderToStaticMarkup(
      <AnimatedCurrency still className="hero-number" value={value} />,
    )
    expect(parse(plain).textContent).toBe(flowText(flow))
    expect(styledParts(parse(plain), (part) => `.${part}`)).toEqual(
      styledParts(flows(flow)[0]!, (part) => `[part~="${part}"]`),
    )
  },
)

it('splits a hidden amount into parts without its digits', () => {
  setAmountsHidden(true)
  const html = renderToStaticMarkup(<AnimatedCurrency className="hero-number" value={1234.5} />)
  expect(html).not.toMatch(/\d/)
  expect(styledParts(parse(html), (part) => `.${part}`)).toEqual(['$', '••••', null])
})

it.each([
  [100, 12.5],
  [-100, -12.5],
  [0, 0],
  [1234567.89, 12345.678],
])('writes a plain change as NumberFlow does for %s (%s%)', (amount, percent) => {
  const flow = renderToStaticMarkup(<ChartChange amount={amount} percent={percent} />)
  const plain = renderToStaticMarkup(<ChartChange still amount={amount} percent={percent} />)
  expect(parse(plain).textContent).toBe(flowText(flow))
})

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

it('keeps action labels visible with an icon and preserves native form semantics', () => {
  const html = renderToStaticMarkup(
    <Button icon={Save} type="submit" disabled aria-busy>
      Save changes
    </Button>,
  )
  expect(html).toContain('type="submit"')
  expect(html).toContain('disabled=""')
  expect(html).toContain('aria-busy="true"')
  expect(html).toContain('Save changes')
  expect(html).not.toContain('class="sr-only"')
  expect(html).toMatch(/<svg[^>]*aria-hidden="true"/)
})

it.each(['icon', 'icon-compact'] as const)('preserves accessible labels for %s buttons', (size) => {
  const html = renderToStaticMarkup(
    <Button icon={Save} size={size}>
      Save changes
    </Button>,
  )
  expect(html).toContain('<span class="sr-only">Save changes</span>')
  expect(html).toMatch(/<svg[^>]*aria-hidden="true"/)
})
