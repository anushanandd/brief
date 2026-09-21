import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import { Treemap, treemapTiles } from './treemap'

it('preserves relative area and partitions the plot without overlaps', () => {
  const segments = [60, 25, 10, 4, 1, 0, -1, NaN].map((value, i) => ({
    name: `Item ${i}`,
    value,
    color: '#aaa',
    changePct: i === 0 ? 5 : null,
    href: `/items/${i}`,
    onSelect: () => undefined,
  }))
  const tiles = treemapTiles(segments)
  expect(tiles).toHaveLength(5)
  expect(tiles.reduce((area, tile) => area + tile.width * tile.height, 0)).toBeCloseTo(10000)
  for (const [index, tile] of tiles.entries()) {
    expect((tile.width * tile.height) / 10000).toBeCloseTo(tile.value / 100)
    expect(tile.x + tile.width).toBeLessThanOrEqual(100.00001)
    expect(tile.y + tile.height).toBeLessThanOrEqual(100.00001)
    for (const other of tiles.slice(index + 1)) {
      const overlapWidth =
        Math.min(tile.x + tile.width, other.x + other.width) - Math.max(tile.x, other.x)
      const overlapHeight =
        Math.min(tile.y + tile.height, other.y + other.height) - Math.max(tile.y, other.y)
      expect(overlapWidth <= 0.00001 || overlapHeight <= 0.00001).toBe(true)
    }
  }
  const html = renderToStaticMarkup(<Treemap segments={segments} label="Synthetic holdings" />)
  expect(html).toContain('href="/items/0"')
  expect(html).toContain('data-slot="tooltip-trigger"')
  expect(html).not.toContain('title=')
  expect(html).toContain('Item 4')
  expect(html).toContain('$1.00')
  expect(html).toContain('one week +5.00%')
  const fullValue = renderToStaticMarkup(
    <Treemap
      segments={[
        {
          name: 'Large value',
          value: 125_000,
          color: '#aaa',
          href: '/items/large',
          onSelect: () => undefined,
        },
      ]}
      label="Full values"
    />,
  )
  expect(fullValue).toContain('$125,000.00')
  expect(treemapTiles([])).toEqual([])
})
