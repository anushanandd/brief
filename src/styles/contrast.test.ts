import { readFileSync } from 'node:fs'
import { join } from 'node:path'

import { describe, expect, it } from 'vitest'

// Each theme's text and series colors are checked against its page and card surfaces, so a
// retuned token can't quietly fall under WCAG: 4.5:1 for the small text the inks carry, 3:1 for
// the bars, dots, and lines drawn in the data colors (1.4.11).
const css = readFileSync(join(process.cwd(), 'src/styles/tokens.css'), 'utf8')

const root = (() => {
  const start = css.indexOf(':root {')
  return css.slice(start, css.indexOf('\n}', start))
})()

/** A token's hex value in a theme, following `var()` references and falling back to the root. */
function resolve(blockText: string, name: string, depth = 0): string {
  const match =
    new RegExp(`${name}:\\s*([^;]+);`).exec(blockText) ??
    new RegExp(`${name}:\\s*([^;]+);`).exec(root)
  if (!match || depth > 5) throw new Error(`${name} is not defined`)
  const value = (match[1] ?? '').trim()
  const [, reference] = /^var\((--[a-z0-9-]+)\)$/.exec(value) ?? []
  if (reference) return resolve(blockText, reference, depth + 1)
  if (!/^#[0-9a-fA-F]{6}$/.test(value)) throw new Error(`${name} is not a hex color: ${value}`)
  return value
}

const channel = (value: number) => {
  const scaled = value / 255
  return scaled <= 0.03928 ? scaled / 12.92 : ((scaled + 0.055) / 1.055) ** 2.4
}

function luminance(color: string) {
  const at = (index: number) => channel(parseInt(color.slice(index, index + 2), 16))
  return 0.2126 * at(1) + 0.7152 * at(3) + 0.0722 * at(5)
}

const contrast = (foreground: string, background: string) => {
  const [first, second] = [luminance(foreground), luminance(background)]
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05)
}

const themes = [{ name: 'Brief', text: root, surfaces: ['#08090a', '#1b1c1e'] }]

describe.each(themes)('$name theme contrast', ({ text, surfaces }) => {
  const tertiary = resolve(text, '--text-tertiary')
  const series = Array.from({ length: 7 }, (_, index) => resolve(text, `--data-${index + 1}`))
  const status = ['--positive', '--negative', '--warning'].map((name) => resolve(text, name))

  it.each(surfaces)('keeps tertiary text at AA on %s', (surface) => {
    expect(contrast(tertiary, surface)).toBeGreaterThanOrEqual(4.5)
  })

  it.each(surfaces)('keeps every data color at 3:1 on %s', (surface) => {
    for (const color of series) expect(contrast(color, surface)).toBeGreaterThanOrEqual(3)
  })

  it.each(surfaces)('keeps status colors readable as text on %s', (surface) => {
    for (const color of status) expect(contrast(color, surface)).toBeGreaterThanOrEqual(4.5)
  })
})
