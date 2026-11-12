import { readFileSync, readdirSync } from 'node:fs'
import { join, relative } from 'node:path'

import { expect, it } from 'vitest'

// A missing amount stays missing (PRINCIPLES.md: true before useful). `amount ?? 0` handed to a
// currency formatter shows an unknown value as $0.00, which reads as a real figure; pass the
// nullable value, which formats as "—", or say why it's unavailable. Lint can't express this
// pattern, so this scans for it. An allowed use names the file and the reason.
const allowed: Record<string, string> = {
  // After its null check, `value || 0` turns NaN and -0 into 0; it never stands for unknown.
  'src/lib/format.ts': 'formatCurrency(value || 0)',
}

const root = join(process.cwd(), 'src')
const sources = (directory: string): string[] =>
  readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name)
    if (entry.isDirectory()) return entry.name === 'bindings' ? [] : sources(path)
    return /\.tsx?$/.test(entry.name) && !/\.test\.tsx?$/.test(entry.name) ? [path] : []
  })

/** The text from `open` to its matching close, or the rest of the source when unbalanced. */
function balanced(source: string, start: number, open: string, close: string) {
  let depth = 0
  for (let index = start; index < source.length; index += 1) {
    if (source[index] === open) depth += 1
    else if (source[index] === close && --depth === 0) return source.slice(start, index + 1)
  }
  return source.slice(start)
}

const zeroFallback = /(\?\?|\|\|)\s*0(?![\w.])/
const formatter = /\bformat\w*Currency\w*\(/g
const animated = /<AnimatedCurrency\b[^>]*?\bvalue=\{/g

function unknownAsZero(source: string) {
  const found: string[] = []
  for (const match of source.matchAll(formatter)) {
    const call = match[0] + balanced(source, match.index + match[0].length - 1, '(', ')').slice(1)
    if (zeroFallback.test(call)) found.push(call.replaceAll(/\s+/g, ' '))
  }
  for (const match of source.matchAll(animated)) {
    const value = balanced(source, match.index + match[0].length - 1, '{', '}')
    if (zeroFallback.test(value)) found.push(`AnimatedCurrency value=${value}`)
  }
  return found
}

it('finds the pattern it guards against', () => {
  expect(unknownAsZero('formatCurrency(total ?? 0)')).toHaveLength(1)
  expect(unknownAsZero('formatCompactCurrency(\n  sum(a) || 0,\n)')).toHaveLength(1)
  expect(unknownAsZero('<AnimatedCurrency className="x" value={balance ?? 0} />')).toHaveLength(1)
  expect(unknownAsZero('formatCurrency(total ?? 0.5)')).toEqual([])
  expect(unknownAsZero('formatCurrency(total)')).toEqual([])
  expect(unknownAsZero('formatCurrency(total ?? null) || 0')).toEqual([])
})

it('never formats an unknown amount as zero', () => {
  const offending = sources(root).flatMap((path) => {
    const file = relative(process.cwd(), path)
    return unknownAsZero(readFileSync(path, 'utf8'))
      .filter((call) => !allowed[file] || !call.includes(allowed[file]))
      .map((call) => `${file}: ${call}`)
  })
  expect(offending).toEqual([])
})
