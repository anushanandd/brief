// @vitest-environment jsdom

import { act, useState } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { pageShortcutBlocked } from '../lib/keyboard'
import { ChartRangeSelect } from './ui'

let root: Root

function RangePreview({ region = true }: { region?: boolean }) {
  const [value, setValue] = useState('week')
  return (
    <div className="surface-card">
      <ChartRangeSelect
        label="Chart range"
        options={[
          { value: 'week', label: 'W', accessibleLabel: 'Week' },
          { value: 'month', label: 'M', accessibleLabel: 'Month' },
        ]}
        value={value}
        onValueChange={setValue}
      />
      {region ? (
        <div role="region" aria-label="Synthetic chart" tabIndex={0} data-range-focus-region />
      ) : null}
      <button>Outside action</button>
    </div>
  )
}

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn()
  root = createRoot(document.body.appendChild(document.createElement('div')))
})

afterEach(async () => {
  await act(async () => root.unmount())
  document.body.replaceChildren()
})

async function key(target: Element, value: string) {
  await act(async () => {
    target.dispatchEvent(
      new KeyboardEvent('keydown', { key: value, bubbles: true, cancelable: true }),
    )
  })
}

async function pointerClick(target: Element) {
  await act(async () => {
    target.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 0, detail: 1 }))
    target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, button: 0, detail: 1 }))
    if (target instanceof HTMLElement) target.focus()
    target.dispatchEvent(new MouseEvent('pointerup', { bubbles: true, button: 0, detail: 1 }))
    target.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, button: 0, detail: 1 }))
    target.dispatchEvent(new MouseEvent('click', { bubbles: true, button: 0, detail: 1 }))
  })
}

it('returns focus to the trigger after keyboard selection', async () => {
  await act(async () => root.render(<RangePreview />))
  const trigger = document.querySelector<HTMLButtonElement>('[role="combobox"]')!
  await act(async () => trigger.focus())
  await key(trigger, 'ArrowDown')
  await act(async () => {
    await vi.waitFor(() => expect(document.querySelector('[role="listbox"]')).not.toBeNull())
  })
  const month = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(
    (option) => option.textContent === 'Month',
  )!
  await act(async () => month.focus())
  await key(month, 'Enter')
  await act(async () => {
    await vi.waitFor(() => expect(document.activeElement).toBe(trigger))
  })
  expect(trigger.textContent).toContain('Month')
})

it('returns focus to the trigger after Escape, including a pointer-opened menu', async () => {
  await act(async () => root.render(<RangePreview />))
  const trigger = document.querySelector<HTMLButtonElement>('[role="combobox"]')!
  await pointerClick(trigger)
  await key(document.querySelector('[role="listbox"]')!, 'Escape')
  await act(async () => {
    await vi.waitFor(() => expect(document.activeElement).toBe(trigger))
  })
  expect(trigger.textContent).toContain('Week')
})

it('focuses the named chart after pointer selection so page shortcuts remain usable', async () => {
  await act(async () => root.render(<RangePreview />))
  const trigger = document.querySelector<HTMLButtonElement>('[role="combobox"]')!
  await pointerClick(trigger)
  const month = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(
    (option) => option.textContent === 'Month',
  )!
  await pointerClick(month)
  await act(async () => {
    await vi.waitFor(() =>
      expect(document.activeElement).toBe(document.querySelector('[data-range-focus-region]')),
    )
  })
  expect(trigger.textContent).toContain('Month')
  expect(pageShortcutBlocked()).toBe(false)
})

it('retains trigger focus when no chart region is available', async () => {
  await act(async () => root.render(<RangePreview region={false} />))
  const trigger = document.querySelector<HTMLButtonElement>('[role="combobox"]')!
  await pointerClick(trigger)
  const month = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(
    (option) => option.textContent === 'Month',
  )!
  await pointerClick(month)
  await act(async () => {
    await vi.waitFor(() => expect(document.activeElement).toBe(trigger))
  })
})

it('leaves focus on an outside control when it dismisses the menu', async () => {
  await act(async () => root.render(<RangePreview />))
  const trigger = document.querySelector<HTMLButtonElement>('[role="combobox"]')!
  const outside = [...document.querySelectorAll<HTMLButtonElement>('button')].find(
    (candidate) => candidate.textContent === 'Outside action',
  )!
  await pointerClick(trigger)
  await pointerClick(outside)
  await act(async () => {
    await vi.waitFor(() => expect(trigger.getAttribute('aria-expanded')).toBe('false'))
  })
  expect(document.activeElement).toBe(outside)
})
