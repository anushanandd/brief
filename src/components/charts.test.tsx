import { Liveline } from 'liveline'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { ChartRangeSelector, DonutChart, PerformanceChart } from './charts'

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] })
  vi.setSystemTime(new Date('2026-09-03T12:00:00Z'))
})
afterEach(() => vi.useRealTimers())

it('labels every nonzero donut segment in descending order', () => {
  const markup = renderToStaticMarkup(
    <DonutChart
      segments={[
        { name: 'Travel', value: 50, color: '#fff' },
        { name: 'Dining', value: 10, color: '#aaa' },
        { name: 'Shopping', value: 10, color: '#999' },
        { name: 'Utilities', value: 10, color: '#888' },
        { name: 'Groceries', value: 10, color: '#777' },
        { name: 'Entertainment', value: 10, color: '#666' },
        { name: 'Empty', value: 0, color: '#000' },
      ]}
      label="Spending by category"
      centerValue="$100"
      centerLabel="spent"
    />,
  )

  expect(markup.match(/class="donut-legend-item"/g)).toHaveLength(6)
  expect(markup.indexOf('>Travel<')).toBeLessThan(markup.indexOf('>Dining<'))
  expect(markup).toContain('>Entertainment<')
  expect(markup).toContain('aria-label="Entertainment: $10.00 (10.0%)"')
  expect(markup).not.toContain('>Empty<')
})

it('exposes account composition in a keyboard-accessible legend', () => {
  const html = renderToStaticMarkup(
    <DonutChart
      segments={[
        {
          name: 'Synthetic account',
          value: 100,
          color: '#fff',
          details: [
            { name: 'TEST', value: 75, color: '#aaa' },
            { name: 'Cash', value: 25, color: '#999' },
          ],
        },
      ]}
      label="Assets"
      centerValue="$100"
      centerLabel="assets"
    />,
  )
  expect(html).toContain('<details')
  expect(html).toContain('<summary')
  expect(html).toContain('>TEST<')
  expect(html).toContain('>Cash<')
  expect(html).toContain('aria-label="$75.00"')
})

it('renders range controls and the regular-market close marker', () => {
  const now = Date.parse('2026-09-03T12:00:00Z') / 1_000
  const points = [
    { time: now - 23 * 60 * 60, value: 100 },
    { time: now, value: 110 },
  ]
  const html = renderToStaticMarkup(
    <>
      <ChartRangeSelector value={0} onValueChange={() => undefined} />
      <PerformanceChart
        data={points}
        netDeposits={points}
        benchmark={points}
        value={110}
        referenceIso="2026-09-03T12:00:00Z"
        selectedWindow={0}
        sessionBoundary={points[0]}
      />
    </>,
  )
  const labels = [...html.matchAll(/<button[^>]*>(.*?)<\/button>/g)].map((match) =>
    match[1].replaceAll(/<[^>]*>/g, ''),
  )
  expect(labels.slice(0, 4)).toEqual(['W', 'M', 'Q', 'A'])
  expect(html).toContain('Chart range: All time')
  expect(html).toContain('aria-label="Regular market close"')
  expect(html).toContain('with the regular-market close marked')
})

it.each([
  [0, 'All time'],
  [30 * 24 * 60 * 60, '1 month'],
])('keeps range %s distinct when all history spans one month', (selectedWindow, label) => {
  const firstTime = 1_788_307_200
  const points = [
    { time: firstTime, value: 100 },
    { time: firstTime + 2_553_120, value: 110 },
  ]
  const html = renderToStaticMarkup(
    <PerformanceChart
      data={points}
      value={110}
      referenceIso="2026-09-03T12:00:00Z"
      selectedWindow={selectedWindow}
    />,
  )
  expect(html).toContain(`Chart range: ${label}`)
  expect(html).not.toContain('aria-label="Regular market close"')
})

it('retains markers in a fixed historical window instead of filtering against today', () => {
  const endTime = 1600000000
  const html = renderToStaticMarkup(
    <Liveline
      data={[
        { time: endTime - 60, value: 100 },
        { time: endTime, value: 101 },
      ]}
      value={101}
      valueTime={endTime}
      endTime={endTime}
      window={120}
      badge={false}
      continuous={false}
      markers={[
        {
          id: 'saved-event',
          time: endTime - 30,
          value: 100.5,
          color: '#fff',
          label: 'Saved transaction',
        },
        { id: 'outside', time: endTime - 1000, value: 90, color: '#fff', label: 'Outside window' },
      ]}
    />,
  )
  expect(html).toContain('aria-label="Saved transaction"')
  expect(html).not.toContain('aria-label="Outside window"')
})

it('uses the same badge-buffered marker window for single and comparison charts', () => {
  const endTime = 1600000000
  const data = [
    { time: endTime - 120, value: 100 },
    { time: endTime, value: 110 },
  ]
  for (const series of [undefined, [{ id: 'Value', data, value: 110, color: '#fff' }]]) {
    const html = renderToStaticMarkup(
      <Liveline
        data={data}
        value={110}
        series={series}
        endTime={endTime}
        window={120}
        badge
        markers={[
          { id: 'inside', time: endTime - 110, value: 100, color: '#fff', label: 'Inside plot' },
          { id: 'outside', time: endTime - 118, value: 100, color: '#fff', label: 'Outside plot' },
        ]}
      />,
    )
    expect(html).toContain('aria-label="Inside plot"')
    expect(html).not.toContain('aria-label="Outside plot"')
  }
})
