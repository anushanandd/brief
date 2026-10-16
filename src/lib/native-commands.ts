import { invoke } from '@tauri-apps/api/core'

import type { ActionBinding, AppAction } from './action-system'
import { isTauri } from './api'
import { asciiMinus } from './format'

/** Menu-bar sections in the order an app's own menus run, before Window and Help. */
const sectionTitles: Record<string, string> = {
  Navigate: 'Go',
  View: 'View',
  Appearance: 'View',
}

export type CommandMenuSection = {
  title: string
  items: Array<{ id: string; title: string; enabled: boolean; accelerator: string | null }>
}

// Shortcuts the static menu already owns, so they never appear twice.
const staticMenuBindings = new Set(['Meta+,', 'Meta+h'])
const acceleratorKeys: Record<string, string> = {
  '/': 'Slash',
  '.': 'Period',
  '[': 'BracketLeft',
  ']': 'BracketRight',
}

/**
 * The menu-bar key equivalent for a binding. Only Command shortcuts that never edit text get
 * one: a menu key equivalent fires even while a field has focus, so bare keys and Command
 * arrows (which move the insertion point) stay with the page, which pauses them during typing.
 * Command-Control arrows edit nothing, so they keep theirs.
 */
export function menuAccelerator(binding: ActionBinding | undefined) {
  if (!binding || binding.sequence.length !== 1) return null
  const parts = binding.sequence[0].split('+')
  const key = parts.at(-1) ?? ''
  const modifiers = parts.slice(0, -1)
  const arrow = key.startsWith('Arrow')
  if (!modifiers.includes('Meta') || (arrow && !modifiers.includes('Control'))) return null
  if (staticMenuBindings.has(binding.sequence[0])) return null
  const named =
    acceleratorKeys[key] ?? (arrow ? key : /^[a-z0-9]$/i.test(key) ? key.toUpperCase() : undefined)
  if (!named) return null
  const shift = /^[A-Z]$/.test(key) ? ['Shift'] : []
  return [
    ...modifiers.map((modifier) =>
      modifier === 'Meta' ? 'Cmd' : modifier === 'Control' ? 'Ctrl' : modifier,
    ),
    ...shift,
    named,
  ].join('+')
}

/**
 * Every app and page command, grouped for the menu bar. List and selection commands follow
 * focus too closely to be menu items; they stay in the palette and the key guide.
 */
export function commandMenuSections(actions: readonly AppAction[]): CommandMenuSection[] {
  const sections = new Map<string, CommandMenuSection>()
  for (const action of actions) {
    if (action.scope !== 'app' && action.scope !== 'page') continue
    if (action.bindings?.some(({ sequence }) => staticMenuBindings.has(sequence.join(' '))))
      continue
    const title = sectionTitles[action.group ?? ''] ?? 'Actions'
    const section = sections.get(title) ?? { title, items: [] }
    section.items.push({
      id: action.id,
      title: action.title,
      enabled: !action.disabled,
      accelerator: menuAccelerator(action.bindings?.find((binding) => menuAccelerator(binding))),
    })
    sections.set(title, section)
  }
  return ['Go', 'View', 'Actions'].flatMap((title) => sections.get(title) ?? [])
}

export async function setCommandMenu(sections: CommandMenuSection[]) {
  if (isTauri()) await invoke('set_command_menu', { sections })
}

export async function copyText(text: string) {
  if (isTauri()) await invoke('copy_text', { text })
  else await navigator.clipboard.writeText(text)
}

/**
 * A row as it reads, one cell per tab, so it pastes into a spreadsheet as a row. Anything
 * screen privacy hides stays out, and so do icons and marks.
 */
export function rowCopyText(row: HTMLElement) {
  // Hidden values leave the text for the read and come straight back: no frame paints between.
  const masked = [...row.querySelectorAll<HTMLElement>('*')]
    .filter((element) => getComputedStyle(element).filter !== 'none')
    .map((element) => [element, element.style.getPropertyValue('display')] as const)
  for (const [element] of masked) element.style.setProperty('display', 'none', 'important')
  try {
    return row.innerText
      .split('\n')
      .map((line) => line.trim())
      .filter((line) => line && line !== '·')
      .join('\t')
  } finally {
    for (const [element, display] of masked) element.style.setProperty('display', display)
  }
}

/** The figure a row stands for, when it has one and privacy isn't hiding it. */
export function rowAmountText(row: HTMLElement) {
  const amount = row.querySelector<HTMLElement>('[data-copy-amount]')
  if (!amount || getComputedStyle(amount).filter !== 'none') return undefined
  // Spreadsheets parse a hyphen, not the typographic minus the screen shows.
  return asciiMinus(amount.innerText.trim()) || undefined
}

/**
 * The in-app location a `brief://` link names, or nothing for one that isn't a page. Links
 * only navigate: Settings opens at a section, and every other path must be a route.
 */
export function openedViewHref(view: string, routes: readonly string[]) {
  if (!view.startsWith('/')) return undefined
  if (view.startsWith('/settings')) return `/#${view.slice(1)}`
  const path = view.split('?')[0]
  return routes.some((route) =>
    route.includes('$') ? path.startsWith(route.slice(0, route.indexOf('$'))) : path === route,
  )
    ? view
    : undefined
}

/** A link back to the view on screen, for pasting into notes, Raycast, or Shortcuts. */
export function viewLink(location: { pathname: string; searchStr?: string }) {
  return `brief://${location.pathname.slice(1)}${location.searchStr ?? ''}`
}
