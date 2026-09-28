import { Liveline } from 'liveline'
import { renderToStaticMarkup } from 'react-dom/server'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { ChartRangeSelector, DonutChart, PerformanceChart, SpendingBarChart } from './charts'

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
  expect(markup).toContain(
    'class="donut-legend" aria-label="Spending by category, all values" tabindex="0"',
  )
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

it('renders accessible category-stacked spending bars with five axis labels', () => {
  const html = renderToStaticMarkup(
    <SpendingBarChart
      label="This week"
      activities={
        new Map([
          [
            'spending:dining',
            {
              id: 'spending:dining',
              kind: 'spending' as const,
              category: 'Dining',
              title: 'Cafe',
              detail: 'Card · Dining',
              date: '2026-09-01',
              amount: -30,
              logoUrl: 'data:image/svg+xml,cafe',
            },
          ],
          [
            'spending:travel',
            {
              id: 'spending:travel',
              kind: 'spending' as const,
              category: 'Travel',
              title: 'Airline',
              detail: 'Card · Travel',
              date: '2026-09-01',
              amount: -10,
              logoUrl: 'data:image/svg+xml,airline',
            },
          ],
          [
            'spending:credit',
            {
              id: 'spending:credit',
              kind: 'credit' as const,
              category: 'Credit',
              title: 'Statement credit',
              detail: 'Card · Credit',
              date: '2026-09-01',
              amount: 5,
            },
          ],
        ])
      }
      categories={[
        { name: 'Dining', color: '#fff' },
        { name: 'Travel', color: '#aaa' },
        { name: 'Credits', color: '#0f0' },
      ]}
      data={[
        {
          from: '2026-09-01',
          to: '2026-09-01',
          value: 35,
          categories: [
            {
              name: 'Dining',
              value: 30,
              activities: [{ id: 'spending:dining', value: 30 }],
            },
            {
              name: 'Travel',
              value: 10,
              activities: [{ id: 'spending:travel', value: 10 }],
            },
            {
              name: 'Credits',
              value: -5,
              activities: [{ id: 'spending:credit', value: 5 }],
            },
          ],
        },
      ]}
    />,
  )
  expect(html).toContain('class="analytics-plot spending-bar-chart"')
  expect(html).toContain('aria-label="Sep 1: $35.00. Dining $30.00, Travel $10.00, Credits $5.00"')
  expect(html).toContain('aria-label="Dining, Sep 1: $30.00"')
  expect(html).toContain('aria-label="Travel, Sep 1: $10.00"')
  expect(html).toContain('aria-label="Credits, Sep 1: $5.00"')
  expect(html.match(/analytics-bar-fill spending-bar-fill/g)).toHaveLength(3)
  expect(html).toContain('spending-bar-fill is-negative')
  expect(html.match(/analytics-bar-axis[\s\S]*?<span/g)).toBeTruthy()
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
  expect(html).toMatch(/role="combobox"[^>]*aria-label="Chart range"/)
  expect(html).toContain('All time')
  expect(html).toContain('Chart range: All time')
  expect(html).toContain('aria-label="Regular market close"')
  expect(html).toContain('with the regular-market close marked')
})

it.each([
  [0, 'All time'],
  [30 * 24 * 60 * 60, 'Month'],
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
