import { renderToStaticMarkup } from 'react-dom/server'
import { expect, it } from 'vitest'

import seedObjects from '../data/seed-objects.json'
import type { ObjectDetail } from '../lib/objects'
import { fromRust } from '../lib/rust-json'
import { holds, ObjectDetailsBody, RawTree } from './object-details'

const graph = fromRust<{
  objects: ObjectDetail['object'][]
  links: ObjectDetail['links']
  facts: ObjectDetail['facts']
}>(seedObjects)

/** Every key anywhere in a stored value whose field holds something. */
function keys(value: unknown): string[] {
  if (Array.isArray(value)) return value.flatMap(keys)
  if (value && typeof value === 'object')
    return Object.entries(value).flatMap(([key, field]) =>
      holds(field) ? [key, ...keys(field)] : [],
    )
  return []
}

const escape = (text: string) =>
  text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;')

// Coverage: the raw view shows every field of every object Brief holds, so nothing stored is
// unreachable however the curated pages change. Rust's own test places every stored field.
it('shows every stored field of every kind of object', () => {
  const kinds = new Set(graph.objects.map(({ kind }) => kind))
  expect(kinds.size).toBeGreaterThanOrEqual(8)
  for (const object of graph.objects) {
    const detail: ObjectDetail = {
      object,
      links: graph.links.filter(({ from, to }) => from === object.id || to === object.id),
      facts: graph.facts.filter(({ subject }) => subject === object.id),
    }
    const html = renderToStaticMarkup(<ObjectDetailsBody detail={detail} onShow={() => {}} />)
    for (const key of new Set(keys(object.record))) {
      expect(html, `${object.id} › ${key}`).toContain(`<dt>${escape(key)}</dt>`)
    }
    for (const fact of detail.facts) expect(html).toContain(escape(fact.predicate))
    for (const link of detail.links) expect(html).toContain(link.relation)
  }
})

it('leaves out fields that hold nothing and keeps a false one', () => {
  const html = renderToStaticMarkup(
    <RawTree value={{ a: null, b: [], c: { d: null, e: '' }, f: true, g: false, h: [null, 0] }} />,
  )
  expect(html).not.toMatch(/<dt>[a-e]<\/dt>/)
  expect(html).toContain('<dt>f</dt><dd><span>yes</span>')
  expect(html).toContain('<dt>g</dt><dd><span>no</span>')
  // A list keeps its items that say something, and a zero says something.
  expect(html).toContain('<dt>h</dt><dd><ol class="raw-list"><li><span>0</span>')
  expect(renderToStaticMarkup(<RawTree value={{ a: null }} />)).toBe('')
})
