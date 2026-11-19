// @vitest-environment jsdom

import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { getActionUsage, type AppAction } from '../lib/action-system'
import { useKeyboardDispatch } from './use-keyboard-dispatch'

const ran: string[] = []
const action = (id: string, scope: AppAction['scope'], sequence: string[]): AppAction => ({
  id,
  title: id,
  scope,
  bindings: [{ sequence, label: sequence.join(' ') }],
  run: () => void ran.push(id),
})
const actions = [
  action('palette.open', 'app', ['Meta+k']),
  action('edits.undo', 'app', ['Meta+z']),
  action('page.export', 'page', ['x']),
]

let root: Root
let region: HTMLElement | undefined
let dialogActions: AppAction[] = []
let pageActions: AppAction[] = []

function Dispatcher() {
  useKeyboardDispatch([...actions, ...dialogActions, ...pageActions], region, false)
  return null
}

/** An open dialog holding a button, with `id` bound to `key` inside it. */
function dialogWith(id: string, key: string) {
  const dialog = document.body.appendChild(document.createElement('dialog'))
  dialog.setAttribute('open', '')
  const button = dialog.appendChild(document.createElement('button'))
  dialogActions.push({ ...action(id, 'dialog', [key]), dialog })
  return { dialog, button }
}

async function mount() {
  await act(async () => root.render(<Dispatcher />))
}

function press(target: Element, key: string, init: KeyboardEventInit = {}) {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
  act(() => void target.dispatchEvent(event))
  return event
}

/** A list of two rows, each with a button, inside `parent`. */
function list(parent: Element) {
  const element = document.createElement('div')
  element.dataset.keyboardRegion = ''
  element.setAttribute('aria-label', 'Rows')
  element.innerHTML =
    '<div data-keyboard-row tabindex="-1"><button>One</button></div>' +
    '<div data-keyboard-row tabindex="-1"><button>Two</button></div>'
  parent.append(element)
  return element
}

beforeEach(() => {
  ran.length = 0
  region = undefined
  dialogActions = []
  pageActions = []
  // jsdom lays nothing out; rows count as visible when they have a box.
  vi.spyOn(HTMLElement.prototype, 'getClientRects').mockReturnValue(
    Object.assign([new DOMRect(0, 0, 10, 10)], { item: () => null }),
  )
  HTMLElement.prototype.scrollIntoView = () => undefined
  root = createRoot(document.body.appendChild(document.createElement('div')))
})

afterEach(async () => {
  await act(async () => root.unmount())
  document.body.replaceChildren()
  vi.restoreAllMocks()
})

it('ranks shortcuts by use, but never list motions', async () => {
  const rows = list(document.body)
  const [first] = rows.querySelectorAll<HTMLElement>('[data-keyboard-row]')
  first?.focus()
  region = rows
  const before = getActionUsage()['page.export']?.count ?? 0
  await mount()
  press(document.body, 'x')
  press(rows, 'j')
  expect(getActionUsage()['page.export']?.count).toBe(before + 1)
  expect(Object.keys(getActionUsage()).some((id) => id.startsWith('motion.'))).toBe(false)
})

it('leaves a key to the page once focus has left the list', async () => {
  const rows = list(document.body)
  const [first] = rows.querySelectorAll<HTMLElement>('[data-keyboard-row]')
  pageActions = [action('page.next', 'page', ['j'])]
  region = rows
  first?.focus()
  first?.blur()
  await mount()
  press(document.body, 'j')
  expect(ran).toEqual(['page.next'])
  expect(document.activeElement).toBe(document.body)
})

it('runs page shortcuts with no dialog open', async () => {
  await mount()
  press(document.body, 'x')
  expect(ran).toEqual(['page.export'])
})

it('runs app actions over an open dialog, but not the page behind it', async () => {
  const dialog = document.body.appendChild(document.createElement('dialog'))
  dialog.setAttribute('open', '')
  const button = dialog.appendChild(document.createElement('button'))
  await mount()
  press(button, 'x')
  expect(ran).toEqual([])
  expect(press(button, 'k', { metaKey: true }).defaultPrevented).toBe(true)
  press(button, 'z', { metaKey: true })
  expect(ran).toEqual(['palette.open', 'edits.undo'])
})

it("steps through a list inside the dialog, and never through the page's list behind it", async () => {
  const behind = list(document.body)
  const dialog = document.body.appendChild(document.createElement('dialog'))
  dialog.setAttribute('open', '')
  const inside = list(dialog)
  const [first, second] = inside.querySelectorAll<HTMLElement>('[data-keyboard-row]')
  if (!first || !second) throw new Error('The dialog lists two rows')
  first.focus()
  region = inside
  await mount()
  press(first, 'j')
  expect(document.activeElement).toBe(second)

  // A stale region left on the page doesn't move while the dialog owns the keyboard.
  const [pageRow] = behind.querySelectorAll<HTMLElement>('[data-keyboard-row]')
  region = behind
  await mount()
  press(second, 'j')
  expect(document.activeElement).toBe(second)
  expect(pageRow).not.toBe(document.activeElement)
})

it("enters a row without an open target, and leaves Return on the row's button to the button", async () => {
  const dialog = document.body.appendChild(document.createElement('dialog'))
  dialog.setAttribute('open', '')
  const inside = list(dialog)
  const [first] = inside.querySelectorAll<HTMLElement>('[data-keyboard-row]')
  const button = first?.querySelector('button')
  if (!first || !button) throw new Error('The dialog lists a row with a button')
  first.focus()
  region = inside
  await mount()
  press(first, 'Enter')
  expect(document.activeElement).toBe(button)
  expect(press(button, 'Enter').defaultPrevented).toBe(false)
})

it('runs a dialog’s own keys only while that dialog holds the keyboard', async () => {
  const below = dialogWith('below.next', 'j')
  await mount()
  press(below.button, 'j')
  expect(ran).toEqual(['below.next'])

  // A dialog opened over it, holding focus, takes the keyboard; the one below stays quiet.
  const above = dialogWith('above.next', 'j')
  above.button.focus()
  await mount()
  press(above.button, 'j')
  expect(ran).toEqual(['below.next', 'above.next'])

  // With no dialog open, nothing scoped to one runs.
  above.dialog.remove()
  below.dialog.remove()
  press(document.body, 'j')
  expect(ran).toEqual(['below.next', 'above.next'])
})

it('runs a dialog’s single key at once, even where it starts a longer shortcut', async () => {
  dialogActions.push(action('cash.first', 'app', ['c', '1']))
  const { button } = dialogWith('inspector.category', 'c')
  await mount()
  press(button, 'c')
  expect(ran).toEqual(['inspector.category'])
})

it('leaves Space and Return on a control inside the dialog to the control', async () => {
  const { button } = dialogWith('viewer.close', 'Space')
  dialogActions.push({
    ...action('viewer.open', 'dialog', ['Enter']),
    dialog: button.parentElement,
  })
  await mount()
  expect(press(button, ' ').defaultPrevented).toBe(false)
  expect(press(button, 'Enter').defaultPrevented).toBe(false)
  expect(ran).toEqual([])
  press(document.body, ' ')
  expect(ran).toEqual(['viewer.close'])
})
