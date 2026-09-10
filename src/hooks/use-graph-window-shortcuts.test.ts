import { describe, expect, it } from 'vitest'

import { graphAccountShortcut, graphWindowShortcut } from './use-graph-window-shortcuts'

const key = (overrides: Partial<Parameters<typeof graphWindowShortcut>[0]> = {}) => ({
  altKey: false,
  ctrlKey: true,
  key: 'ArrowRight',
  metaKey: true,
  shiftKey: false,
  ...overrides,
})

describe('graphWindowShortcut', () => {
  it('requires Command and Control without treating Command alone as a date-view shortcut', () => {
    expect(graphWindowShortcut(key())).toBe('graph-window-next')
    expect(graphWindowShortcut(key({ key: 'ArrowLeft' }))).toBe('graph-window-previous')
    expect(graphWindowShortcut(key({ ctrlKey: false }))).toBeUndefined()
    expect(graphWindowShortcut(key({ metaKey: false }))).toBeUndefined()
    expect(graphAccountShortcut(key())).toBeUndefined()
    expect(graphAccountShortcut(key({ ctrlKey: false }))).toBe('graph-next')
  })
})
