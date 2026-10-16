import { expect, it } from 'vitest'

import { isPinned, MAX_PINS, parsePins, withPin, type Pin } from './pins'

const pin = (id: string, args: Pin['arguments'] = {}): Pin => ({
  id,
  question: `Question ${id}`,
  name: 'cashFlow',
  arguments: args,
})

it('keeps one pin per query, newest first, and drops the oldest past the limit', () => {
  let pins: Pin[] = []
  for (let index = 0; index < MAX_PINS + 2; index += 1)
    pins = withPin(pins, pin(`p${index}`, { start: `2026-0${index + 1}-01` }))
  expect(pins).toHaveLength(MAX_PINS)
  expect(pins[0].id).toBe(`p${MAX_PINS + 1}`)
  // Pinning the same query again replaces it instead of adding a second card.
  const repinned = withPin(pins, pin('again', pins[2].arguments))
  expect(repinned).toHaveLength(MAX_PINS)
  expect(repinned[0].id).toBe('again')
  expect(isPinned(repinned, 'cashFlow', pins[2].arguments)).toBe(true)
  expect(isPinned(repinned, 'cashFlow', { start: '1999-01-01' })).toBe(false)
})

it('reads saved pins defensively', () => {
  expect(parsePins(null)).toEqual([])
  expect(parsePins('{broken')).toEqual([])
  expect(parsePins(JSON.stringify([pin('ok'), { id: 1 }, 'text']))).toEqual([pin('ok')])
})
