import { z } from 'zod'

import { preference } from './preference'

/**
 * Ledger queries kept on Today. A pin stores the question and the query, never its figures:
 * native code recomputes them from saved data each time, without the language model.
 */
const pinSchema = z.object({
  id: z.string(),
  question: z.string().max(1_000),
  name: z.string().max(64),
  arguments: z.record(z.string(), z.unknown()),
})
export type Pin = z.infer<typeof pinSchema>

export const MAX_PINS = 6

export function parsePins(raw: string | null): Pin[] {
  try {
    const parsed = z.array(z.unknown()).parse(JSON.parse(raw ?? '[]'))
    return parsed
      .flatMap((item) => {
        const pin = pinSchema.safeParse(item)
        return pin.success ? [pin.data] : []
      })
      .slice(0, MAX_PINS)
  } catch {
    return []
  }
}

const sameQuery = (pin: Pin, name: string, args: Pin['arguments']) =>
  pin.name === name && JSON.stringify(pin.arguments) === JSON.stringify(args)

/** Adds a pin, newest first; the same query is kept once and the oldest falls off the end. */
export function withPin(pins: Pin[], pin: Pin) {
  return [pin, ...pins.filter((other) => !sameQuery(other, pin.name, pin.arguments))].slice(
    0,
    MAX_PINS,
  )
}

export const isPinned = (pins: Pin[], name: string, args: Pin['arguments']) =>
  pins.some((pin) => sameQuery(pin, name, args))

const pinned = preference('brief.pinnedQueries', parsePins, JSON.stringify)
export const savePins = pinned.save
export const usePins = pinned.use
