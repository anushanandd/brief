// @vitest-environment jsdom

import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { afterEach, expect, it, vi } from 'vitest'

import { saveExternalLogosEnabled } from '../lib/logos'
import { SecurityMark } from './security-mark'

afterEach(() => {
  act(() => saveExternalLogosEnabled(false))
  vi.unstubAllGlobals()
})

it('follows the Logos setting without remounting', () => {
  vi.stubGlobal('IS_REACT_ACT_ENVIRONMENT', true)
  const container = document.createElement('div')
  const root = createRoot(container)
  act(() => root.render(<SecurityMark ticker="TEST" />))
  expect(container.querySelector('img')?.src).toMatch(/^data:image\/svg/)
  act(() => saveExternalLogosEnabled(true))
  expect(container.querySelector('img')?.src).toContain('img.logo.dev/ticker/TEST')
  act(() => root.unmount())
})
