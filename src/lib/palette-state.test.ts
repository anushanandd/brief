import { expect, it } from 'vitest'

import { initialPalette, paletteReducer, type PaletteEvent } from './palette-state'

const run = (...events: PaletteEvent[]) => events.reduce(paletteReducer, initialPalette('search'))
const chip = { field: 'account' as const, value: 'card', label: 'Card' }

it('opens the filter picker with a leading slash and jumps to a tab by its prefix', () => {
  expect(run({ type: 'type', query: '/' })).toMatchObject({ filterStep: 'field', query: '' })
  expect(run({ type: 'type', query: '>refresh' })).toMatchObject({
    tabId: 'commands',
    query: 'refresh',
  })
  // A prefix only counts at the start of an empty query.
  expect(run({ type: 'type', query: 'a' }, { type: 'type', query: 'a>' })).toMatchObject({
    tabId: 'all',
    query: 'a>',
  })
})

it('adds one chip per field, switching to Activity, and steps back one level at a time', () => {
  const filtered = run(
    { type: 'chooseField', field: 'account' },
    { type: 'addChip', chip },
    { type: 'addChip', chip: { ...chip, value: 'cash', label: 'Cash' } },
  )
  expect(filtered).toMatchObject({ tabId: 'activity', filterStep: undefined })
  expect(filtered.chips.map(({ label }) => label)).toEqual(['Cash'])
  expect(run({ type: 'chooseField', field: 'date' }, { type: 'back' })).toMatchObject({
    filterStep: 'field',
  })
  expect(paletteReducer(filtered, { type: 'back' }).chips).toEqual([])
})

it('keeps the selection while asking and clears it whenever the results change', () => {
  const selected = run({ type: 'select', id: 'row' })
  expect(paletteReducer(selected, { type: 'type', query: 'x' }).selected).toBe('')
  const asking = paletteReducer(selected, { type: 'switchMode', mode: 'ask', query: 'why' })
  expect(asking).toMatchObject({ mode: 'ask', query: 'why', selected: '' })
  const typed = paletteReducer({ ...asking, selected: 'kept' }, { type: 'type', query: '/' })
  expect(typed).toMatchObject({ query: '/', selected: 'kept', filterStep: undefined })
  expect(run({ type: 'stepTab', offset: -1 }).tabId).not.toBe('all')
})
