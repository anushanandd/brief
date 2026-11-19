// @vitest-environment jsdom
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { styles } from '@number-flow/react'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, expect, it, vi } from 'vitest'

import { setAmountsHidden } from '../lib/format'
import { Save } from './icons'
import {
  AnimatedCurrency,
  Button,
  Change,
  ChartChange,
  ChartRangeSelect,
  hasMoreBelow,
  ScrollCueCard,
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

it('takes back the room NumberFlow keeps around its digits, so layouts space figures', () => {
  // Its fallback text pads by the whole room, which its shadow splits over two spans.
  const room = /number-flow-react > span\{[^}]*padding:calc\((.+?) \* 2\) 0\}/.exec(styles[1])?.[1]
  expect(room).toBeDefined()
  expect(styles[2]).toContain(`.number__inner{padding:${room} `)
  expect(styles[2]).toContain(`.digit__num,.number .section::after{padding:${room} 0}`)
  const css = readFileSync(join(process.cwd(), 'src/styles/components.css'), 'utf8')
  expect(css).toContain(`number-flow-react {\n  margin-block: calc(${room} * -2);\n}`)
  // Each style for a part also styles the plain figure's span of that name.
  const parts = [...css.matchAll(/([\w.-]+)::part\((\w+)\)/g)]
  expect(parts.length).toBeGreaterThan(0)
  for (const [, host, part] of parts) expect(css).toContain(`${host} .${part}`)
})

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

const card = (loaded: boolean) => (
  <ScrollCueCard scrollSelector=".list-scroll">
    {loaded ? (
      <div className="list-scroll">
        <ul>
          <li>Row</li>
        </ul>
      </div>
    ) : (
      <p>Loading</p>
    )}
  </ScrollCueCard>
)

it('finds a scroll region that appears after the card first renders', async () => {
  vi.stubGlobal(
    'ResizeObserver',
    class {
      observe() {}
      unobserve() {}
      disconnect() {}
    },
  )
  // jsdom lays nothing out, so the region reports a list taller than itself.
  vi.spyOn(HTMLElement.prototype, 'scrollHeight', 'get').mockReturnValue(300)
  vi.spyOn(HTMLElement.prototype, 'clientHeight', 'get').mockReturnValue(200)
  const root = createRoot(document.body.appendChild(document.createElement('div')))
  await act(async () => root.render(card(false)))
  await act(async () => root.render(card(true)))
  expect(document.querySelector('.list-scroll')?.hasAttribute('data-more-below')).toBe(true)
  await act(async () => root.unmount())
  document.body.replaceChildren()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
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
