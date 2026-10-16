import { expect, it } from 'vitest'

import { keyCaps } from './keys'

it('gives each modifier and key its own cap', () => {
  expect(keyCaps('⌘K')).toEqual([['⌘', 'K']])
  expect(keyCaps('⇧⌘O')).toEqual([['⇧', '⌘', 'O']])
  expect(keyCaps('⌘^←')).toEqual([['⌘', '⌃', '←']])
  expect(keyCaps('⌥↵')).toEqual([['⌥', '↵']])
})

it('keeps sequences as separate chords and plain keys whole', () => {
  expect(keyCaps('g f')).toEqual([['g'], ['f']])
  expect(keyCaps('g s e')).toEqual([['g'], ['s'], ['e']])
  expect(keyCaps('esc')).toEqual([['Esc']])
  expect(keyCaps('?')).toEqual([['?']])
  expect(keyCaps('⌘')).toEqual([['⌘']])
})
