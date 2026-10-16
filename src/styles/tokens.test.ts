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
    return [...css.matchAll(/(?<=[{;]\s*(?:\/\*[^*]*\*\/\s*)*)([a-z-]+)\s*:([^;{}]+);([^\n]*)/g)]
      .filter((match) => !match[3].includes('scale-exempt'))
      .map((match) => ({
        at: `${name}:${css.slice(0, match.index).split('\n').length}`,
        line: `${match[1]}:${match[2].replaceAll(/\s+/g, ' ')}`,
      }))
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
