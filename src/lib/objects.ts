import type { useNavigate } from '@tanstack/react-router'
import { invoke } from '@tauri-apps/api/core'

import { isTauri, openExternalUrl } from './api'
import type { GraphFact } from './bindings/GraphFact'
import type { GraphLink } from './bindings/GraphLink'
import type { GraphObject } from './bindings/GraphObject'
import type { ObjectDetail } from './bindings/ObjectDetail'
import type { ObjectKind } from './bindings/ObjectKind'
import type { SearchHit } from './bindings/SearchHit'
import type { SearchQuery } from './bindings/SearchQuery'
import { fromRust } from './rust-json'

/**
 * Brief's objects, as Rust's graph holds them: one search over every kind, and any object's
 * complete record for its raw view. The preview reads the seed's graph, made by the same code.
 */
export type { GraphFact, GraphLink, GraphObject, ObjectDetail, ObjectKind, SearchHit }

/** How each kind is named in lists, and the `@` prefix that limits a search to it. */
export const objectKinds: Record<ObjectKind, { label: string; plural: string; prefix: string }> = {
  account: { label: 'Account', plural: 'Accounts', prefix: 'account' },
  transaction: { label: 'Transaction', plural: 'Activity', prefix: 'transaction' },
  holding: { label: 'Holding', plural: 'Holdings', prefix: 'holding' },
  trade: { label: 'Trade', plural: 'Trades', prefix: 'trade' },
  document: { label: 'Document', plural: 'Documents', prefix: 'doc' },
  statement: { label: 'Statement', plural: 'Statements', prefix: 'statement' },
  person: { label: 'Person', plural: 'People', prefix: 'person' },
  organization: { label: 'Organization', plural: 'Organizations', prefix: 'org' },
  place: { label: 'Place', plural: 'Places', prefix: 'place' },
  trip: { label: 'Trip', plural: 'Trips', prefix: 'trip' },
  bill: { label: 'Bill', plural: 'Bills', prefix: 'bill' },
  benefit: { label: 'Benefit', plural: 'Benefits', prefix: 'benefit' },
  story: { label: 'Story', plural: 'News', prefix: 'news' },
  chapter: { label: 'Chapter', plural: 'Chapters', prefix: 'chapter' },
  program: { label: 'Rule', plural: 'Rules', prefix: 'rule' },
}

const isKind = (name: string): name is ObjectKind => name in objectKinds
const kindNames = Object.keys(objectKinds).filter(isKind)

/**
 * A first word naming a kind, then a space, limits a search to that kind: `doc w-2` (or
 * `@doc w-2`) searches documents for "w-2". A word that names no kind is searched for.
 */
export function objectQuery(text: string): { kinds: ObjectKind[]; text: string } {
  const match = /^@?(\w+)\s+(.*)$/s.exec(text.trimStart())
  if (!match) return { kinds: [], text }
  const word = match[1].toLowerCase()
  const kinds = kindNames.filter(
    (kind) =>
      kind.startsWith(word) ||
      objectKinds[kind].prefix.startsWith(word) ||
      objectKinds[kind].plural.toLowerCase().startsWith(word),
  )
  return kinds.length ? { kinds, text: match[2] } : { kinds: [], text }
}

type SeedGraph = { objects: GraphObject[]; links: GraphLink[]; facts: GraphFact[] }

const seedGraph = async () =>
  fromRust<SeedGraph>((await import('../data/seed-objects.json')).default)

const words = (text: string) =>
  text
    .toLowerCase()
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)

/**
 * The preview's search: every query word begins a word of the object's title or subtitle.
 * Without words, a kind lists its objects, the most recent first, as Rust does.
 */
function previewSearch(graph: SeedGraph, query: SearchQuery): SearchHit[] {
  const wanted = words(query.text)
  const ofKind = graph.objects.filter(
    (object) => !query.kinds.length || query.kinds.includes(object.kind),
  )
  return (
    wanted.length
      ? ofKind.filter((object) => {
          const have = words(`${object.title} ${object.subtitle ?? ''}`)
          return wanted.every((word) => have.some((candidate) => candidate.startsWith(word)))
        })
      : ofKind.toSorted((left, right) => (right.date ?? '').localeCompare(left.date ?? ''))
  )
    .slice(0, query.limit ?? 40)
    .map(({ id, kind, title, subtitle, date, amount }) => ({
      id,
      kind,
      title,
      ...(subtitle === undefined ? {} : { subtitle }),
      ...(date === undefined ? {} : { date }),
      ...(amount === undefined ? {} : { amount }),
      match: 'words' as const,
    }))
}

/**
 * Every object matching a query, best first: by words, then by meaning. Without words, the
 * kinds it names, listed.
 */
export async function searchObjects(query: SearchQuery): Promise<SearchHit[]> {
  if (!query.text.trim() && !query.kinds.length) return []
  if (!isTauri()) return previewSearch(await seedGraph(), query)
  return invoke<SearchHit[]>('search_objects', { query })
}

/** One object with its links, facts, and provider record; null when there is none. */
export async function getObject(id: string): Promise<ObjectDetail | null> {
  if (!isTauri()) {
    const graph = await seedGraph()
    const object = graph.objects.find((candidate) => candidate.id === id)
    return object
      ? {
          object,
          links: graph.links.filter((link) => link.from === id || link.to === id),
          facts: graph.facts.filter((fact) => fact.subject === id),
        }
      : null
  }
  return invoke<ObjectDetail | null>('get_object', { id })
}

/** The object an Activity entry is: a transaction (`spending:…`) or a trade (`trade:…`). */
export function activityObjectId(activityId: string): string | undefined {
  if (activityId.startsWith('spending:'))
    return `transaction:${activityId.slice('spending:'.length)}`
  if (activityId.startsWith('trade:')) return activityId
  return undefined
}

const keyOf = (id: string) => id.slice(id.indexOf(':') + 1)
const text = (value: unknown) => (typeof value === 'string' ? value : undefined)

/**
 * Opens the page that shows an object: its account, holding, document, trip, or person, the
 * Activity it names, or a story's link. An object with no page of its own opens its raw view.
 */
export function openObject(
  navigate: ReturnType<typeof useNavigate>,
  object: Pick<GraphObject, 'id' | 'kind' | 'title' | 'date'> & { record?: GraphObject['record'] },
  showDetails: (id: string) => void,
) {
  const key = keyOf(object.id)
  const day = object.date?.slice(0, 10)
  switch (object.kind) {
    case 'account':
      return navigate({ to: '/accounts', search: { account: key } })
    case 'transaction':
      return navigate({
        to: '/activities',
        search: { q: `description:${object.title}`, ...(day ? { from: day, to: day } : {}) },
      })
    case 'holding':
      return navigate({ to: '/holdings/$ticker', params: { ticker: object.title } })
    case 'trade': {
      const ticker = text(object.record?.ticker)
      return ticker
        ? navigate({ to: '/holdings/$ticker', params: { ticker } })
        : navigate({ to: '/activities', search: { q: object.title } })
    }
    case 'document':
      return navigate({ to: '/documents', search: { document: key } })
    case 'statement': {
      const account = text(object.record?.accountId)
      return account ? navigate({ to: '/accounts', search: { account } }) : showDetails(object.id)
    }
    case 'person':
      return object.id === 'person:you'
        ? showDetails(object.id)
        : navigate({ to: '/insights', search: { view: 'people', person: object.id } })
    case 'trip':
      return navigate({ to: '/insights', search: { trip: key } })
    case 'organization':
    case 'place':
      return navigate({ to: '/activities', search: { q: object.title } })
    case 'bill':
      return navigate({ to: '/spending' })
    case 'benefit':
      return navigate({ to: '/analytics', search: { chart: 'amex-credits' } })
    case 'story': {
      const url = text(object.record?.url)
      return url ? openExternalUrl(url) : showDetails(object.id)
    }
    case 'chapter':
      return navigate({ to: '/insights', search: { view: 'chapters', chapter: key } })
    case 'program':
      return navigate({ to: '/insights', search: { view: 'rules' } })
  }
}
