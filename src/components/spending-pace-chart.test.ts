import { expect, it } from 'vitest'

import { dragRange } from './spending-pace-chart'

it('turns a drag into inclusive dates in order, and a click into nothing', () => {
  expect(dragRange('2026-09-01', 2, 9)).toEqual({ from: '2026-09-03', to: '2026-09-10' })
  // Dragging right to left selects the same days.
  expect(dragRange('2026-09-01', 9, 2)).toEqual({ from: '2026-09-03', to: '2026-09-10' })
  // Across a month boundary.
  expect(dragRange('2026-08-25', 5, 8)).toEqual({ from: '2026-08-30', to: '2026-09-02' })
  expect(dragRange('2026-09-01', 4, 4)).toBeUndefined()
})
