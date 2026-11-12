// @vitest-environment jsdom

import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'

import { ActionProvider, useRegisteredActions, type AppAction } from '../../lib/action-system'
import { Dialog, dialogAutofocus } from './dialog'

let root: Root
let registered: readonly AppAction[] = []
const onClose = vi.fn()
const step: AppAction = {
  id: 'inspector.next',
  title: 'Next',
  scope: 'page',
  bindings: [{ sequence: ['j'], label: 'J' }],
  run: () => undefined,
}
const actions = [step]

function Registry() {
  registered = useRegisteredActions()
  return null
}

async function render(open: boolean) {
  await act(async () =>
    root.render(
      <ActionProvider>
        <Registry />
        <Dialog open={open} onClose={onClose} escapeOnKeyDown actions={actions} aria-label="Test">
          <div role="listbox">
            <button type="button">Option</button>
          </div>
          <button type="button">Plain</button>
        </Dialog>
      </ActionProvider>,
    ),
  )
}

const dialog = () => document.querySelector('dialog')!
const escape = () => new KeyboardEvent('keydown', { key: 'Escape', bubbles: true })

// jsdom has no top layer; opening and closing set the attribute.
function showModal(this: HTMLDialogElement) {
  this.setAttribute('open', '')
}
function close(this: HTMLDialogElement) {
  this.removeAttribute('open')
}

beforeEach(() => {
  onClose.mockClear()
  HTMLDialogElement.prototype.showModal = showModal
  HTMLDialogElement.prototype.close = close
  root = createRoot(document.body.appendChild(document.createElement('div')))
})

afterEach(async () => {
  await act(async () => root.unmount())
  document.body.replaceChildren()
})

it('opens with `open`, and registers its keys scoped to itself only while open', async () => {
  await render(false)
  expect(dialog().open).toBe(false)
  expect(registered).toEqual([])
  await render(true)
  expect(dialog().open).toBe(true)
  expect(registered).toMatchObject([{ id: 'inspector.next', scope: 'dialog' }])
  expect(registered[0]?.dialog).toBe(dialog())
  await render(false)
  expect(dialog().open).toBe(false)
  expect(registered).toEqual([])
})

it('focuses its marked control once it has opened, even when it mounts open', async () => {
  // Like WebKit, opening focuses the first control; the marked one must win.
  HTMLDialogElement.prototype.showModal = function () {
    showModal.call(this)
    this.querySelector('button')?.focus()
  }
  await act(async () =>
    root.render(
      <ActionProvider>
        <Dialog open onClose={onClose} aria-label="Test">
          <button type="button">Mode</button>
          <input aria-label="Query" {...dialogAutofocus} />
        </Dialog>
      </ActionProvider>,
    ),
  )
  expect(dialog().open).toBe(true)
  expect(document.activeElement).toBe(document.querySelector('input'))
})

it('closes on cancel, Escape, and a press on the backdrop, but leaves Escape to a menu', async () => {
  await render(true)
  const [option, plain] = dialog().querySelectorAll('button')
  if (!option || !plain) throw new Error('The dialog has two buttons')
  act(() => void dialog().dispatchEvent(new Event('cancel', { cancelable: true })))
  expect(onClose).toHaveBeenCalledTimes(1)

  act(() => void option.dispatchEvent(escape()))
  expect(onClose).toHaveBeenCalledTimes(1)
  act(() => void plain.dispatchEvent(escape()))
  expect(onClose).toHaveBeenCalledTimes(2)

  // A press that starts inside and ends on the dialog element isn't a backdrop click.
  act(() => void option.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true })))
  act(() => void dialog().dispatchEvent(new MouseEvent('click', { bubbles: true })))
  expect(onClose).toHaveBeenCalledTimes(2)
  act(() => void dialog().dispatchEvent(new MouseEvent('pointerdown', { bubbles: true })))
  act(() => void dialog().dispatchEvent(new MouseEvent('click', { bubbles: true })))
  expect(onClose).toHaveBeenCalledTimes(3)
})
