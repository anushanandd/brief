import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'

import { expect, it } from 'vitest'

// Values come from the tokens in tokens.css; these checks keep page styles from drifting off
// them. A declaration that must leave the scale says why in a `scale-exempt` comment.
const directory = join(process.cwd(), 'src/styles')
const declarations = readdirSync(directory)
  .filter((name) => name.endsWith('.css') && name !== 'tokens.css')
  .flatMap((name) => {
    const css = readFileSync(join(directory, name), 'utf8')
    // Whole declarations, including values that run over several lines.
    return (
      [...css.matchAll(/(?<=[{;]\s*(?:\/\*[^*]*\*\/\s*)*)([a-z-]+)\s*:([^;{}]+);([^\n]*)/g)]
        // Each capture group always matches, possibly empty.
        .flatMap(({ 1: property = '', 2: value = '', 3: comment = '', index }) =>
          comment.includes('scale-exempt')
            ? []
            : [
                {
                  at: `${name}:${css.slice(0, index).split('\n').length}`,
                  line: `${property}:${value.replaceAll(/\s+/g, ' ')}`,
                },
              ],
        )
    )
  })
const property = (line: string) => line.slice(0, line.indexOf(':')).trim()
const value = (line: string) => line.slice(line.indexOf(':') + 1).replace(/;$/, '')
const pixels = (text: string) =>
  [...text.matchAll(/(?<![\w.-])-?(\d+(?:\.\d+)?)px/g)].map((match) => Number(match[1]))
const offending = (test: (property: string, value: string) => boolean) =>
  declarations
    .filter(({ line }) => test(property(line), value(line)))
    .map(({ at, line }) => `${at}  ${line}`)

it('spaces on the 4 px scale', () => {
  const scale = new Set([0, 1, 2, 4, 6, 8, 10, 12, 16, 20, 24, 28, 32, 40, 48, 56, 64, 72, 80, 96])
  expect(
    offending(
      (name, text) =>
        /^(gap|row-gap|column-gap|margin|padding|inset)(-|$)/.test(name) &&
        pixels(text).some((size) => !scale.has(size)),
    ),
  ).toEqual([])
})

it('sets type from the type scale', () => {
  expect(
    offending((name, text) => name === 'font-size' && !/var\(--type-|em\b|%|inherit/.test(text)),
  ).toEqual([])
  expect(
    offending(
      (name, text) => name === 'font-weight' && !/^\s*(400|500|600|inherit)\s*$/.test(text),
    ),
  ).toEqual([])
})

it('rounds corners with radius tokens', () => {
  expect(
    offending(
      (name, text) =>
        name.startsWith('border') &&
        name.endsWith('radius') &&
        // Nested corners derive from a token (`calc(var(--radius-card) - 4px)`).
        !text.includes('var(') &&
        pixels(text).some((size) => size !== 1 && size !== 999 && size !== 0),
    ),
  ).toEqual([])
})

it('times motion and colours surfaces with tokens', () => {
  expect(
    // Reduced motion collapses durations to 0.01ms.
    offending(
      (name, text) => /^(transition|animation)/.test(name) && /(?<![\d.])[1-9]\d*ms/.test(text),
    ),
  ).toEqual([])
  expect(
    offending(
      (name, text) =>
        !name.startsWith('mask') && !name.startsWith('--') && /#[\da-f]{3,8}\b|rgba?\(/i.test(text),
    ),
  ).toEqual([])
})

// Table and list text never clips and no table scrolls sideways: cells wrap, and a narrow
// table drops columns with a container query. Only chrome (tabs, menus, headers of a dialog)
// and charts may truncate or scroll; each is listed here by its selector.
const clippingAllowed = new Set([
  // Dialog and viewer titles; the full name is in the body beneath.
  '.transaction-details-header h2',
  '.documents-viewer-header strong',
  '.documents-viewer-header small',
  '.documents-details-disclosure > summary > small',
  // Controls: segments, selects, saved views, and the palette's tabs, header, and items.
  '.activity-filter-segments > button',
  '.ledger-select-value',
  '.ledger-select-item-text',
  '.activity-saved-view > button',
  '.command-tabs',
  '.command-menu-header strong, .command-menu-header small',
  '.command-menu-popover > button > span:first-child',
  '.command-group [cmdk-item] > span',
  '.command-item-copy > span, .command-item-copy > small',
  '.key-guide-title',
  // The selector strip under a page header, whose cards carry a title.
  '.account-switcher-grid',
  '.account-switcher-copy > span, .account-switcher-copy > strong',
  // Charts: bars, legends, flow labels, and a timeline's segments.
  '.analytics-bars-scroll',
  '.bar-chart-legend',
  '.chapters-bar > span',
  '.sankey-label strong',
  '.monthly-bar-column > strong, .monthly-bar-column > small',
])

it('wraps table and list text rather than clipping or scrolling it sideways', () => {
  const clipping = readdirSync(directory)
    .filter((name) => name.endsWith('.css'))
    .flatMap((name) => {
      const css = readFileSync(join(directory, name), 'utf8').replaceAll(/\/\*[\s\S]*?\*\//g, '')
      return [...css.matchAll(/([^{}@]+)\{([^{}]*)\}/g)].flatMap(([, selector = '', body = '']) =>
        /text-overflow:\s*ellipsis|line-clamp|overflow-x:\s*(auto|scroll)/.test(body)
          ? [{ name, selector: selector.trim().replaceAll(/\s+/g, ' ') }]
          : [],
      )
    })
  expect(
    clipping
      .filter(({ selector }) => !clippingAllowed.has(selector))
      .map(({ name, selector }) => `${name}  ${selector}`),
  ).toEqual([])
  // A min-width wider than the 860 px window's content would force a sideways scroll. The window
  // itself has that minimum, and the ledger's measured width (set before paint, dropping columns
  // to fit) only falls back to one.
  expect(
    offending(
      (name, text) =>
        name === 'min-width' &&
        !text.includes('var(--activity-grid-width') &&
        pixels(text).some((size) => size > 560),
    ).filter((line) => !line.startsWith('base.css')),
  ).toEqual([])
})
