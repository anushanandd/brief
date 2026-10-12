import { describe, expect, it, vi } from 'vitest'

import {
  hintCode,
  longestCommonPrefix,
  paletteCompletion,
  resolveBinding,
  type AppAction,
} from './action-system'

const action = (id: string, scope: AppAction['scope'], sequence: string[]): AppAction => ({
  id,
  title: id,
  scope,
  bindings: [{ sequence, label: sequence.join(' ') }],
  run: vi.fn(),
})

describe('action key resolution', () => {
  it('keeps a prefix pending and resolves its completed action', () => {
    const actions = [action('home', 'app', ['g', 'h']), action('focus', 'app', ['g', 'f'])]
    expect(resolveBinding(actions, ['g'])).toMatchObject({ exact: undefined, pending: true })
    expect(resolveBinding(actions, ['g', 'h']).exact?.action.id).toBe('home')
  })

  it('gives the more local scope precedence for an identical binding', () => {
    const actions = [action('global-next', 'app', ['j']), action('row-next', 'region', ['j'])]
    expect(resolveBinding(actions, ['j']).exact?.action.id).toBe('row-next')
  })

  it('does not report unrelated input as a pending command', () => {
    expect(resolveBinding([action('home', 'app', ['g', 'h'])], ['z'])).toEqual({
      candidates: [],
      exact: undefined,
      pending: false,
    })
  })
})

describe('palette completion', () => {
  it('offers a full ghost completion and accepts one semantic segment at a time', () => {
    expect(paletteCompletion('sw', ['Switch to Dark theme'])).toEqual({
      full: 'Switch to Dark theme',
      step: 'Switch ',
    })
    expect(paletteCompletion('Switch ', ['Switch to Dark theme'])?.step).toBe('Switch to ')
  })

  it('only fills the shared unambiguous prefix', () => {
    expect(longestCommonPrefix(['Open Accounts', 'Open Activity'])).toBe('Open Ac')
    expect(paletteCompletion('Open ', ['Open Accounts', 'Open Activity'])).toEqual({
      full: 'Open Ac',
      step: 'Open Ac',
    })
    expect(paletteCompletion('dark', ['Switch to Dark theme'])).toBeUndefined()
  })
})

it('assigns short deterministic focus hints', () => {
  expect(hintCode(0, 4)).toBe('a')
  expect(hintCode(3, 4)).toBe('f')
  expect(hintCode(0, 30)).toHaveLength(2)
  expect(hintCode(29, 30)).not.toBe(hintCode(0, 30))
})
