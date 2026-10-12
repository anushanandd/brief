import { dayMs } from './dates'
import { preference } from './preference'

export type MatchRange = readonly [start: number, end: number]

export type PaletteMatch = {
  score: number
  ranges: readonly MatchRange[]
}

// Tiers follow Raycast's root search: exact and alias hits, then title prefixes and word
// starts, then keywords. Loose substring matches only count once a query is specific enough.
export const matchTier = {
  exact: 100,
  alias: 95,
  aliasPrefix: 85,
  prefix: 80,
  wordStart: 60,
  allTermsInTitle: 55,
  acronym: 50,
  keyword: 40,
  substring: 20,
  keywordSubstring: 10,
} as const

const noMatch: PaletteMatch = { score: 0, ranges: [] }

const isWordChar = (character: string | undefined) =>
  character !== undefined && /[\p{L}\p{N}]/u.test(character)

function wordStartIndex(text: string, term: string) {
  let index = text.indexOf(term)
  while (index >= 0) {
    if (index === 0 || !isWordChar(text[index - 1])) return index
    index = text.indexOf(term, index + 1)
  }
  return -1
}

function acronymRanges(text: string, query: string) {
  const ranges: MatchRange[] = []
  for (let index = 0; index < text.length && ranges.length < query.length; index += 1) {
    if (!isWordChar(text[index]) || (index > 0 && isWordChar(text[index - 1]))) continue
    // Word initials may be skipped, so "sdt" matches "Switch to Dark theme".
    if (text[index] === query[ranges.length]) ranges.push([index, index + 1])
  }
  return ranges.length === query.length ? ranges : undefined
}

export function matchPaletteItem(
  query: string,
  title: string,
  {
    keywords = [],
    aliases = [],
  }: { keywords?: readonly string[]; aliases?: readonly string[] } = {},
): PaletteMatch {
  const needle = query.trim().toLowerCase()
  if (!needle) return noMatch
  const text = title.toLowerCase()
  if (text === needle) return { score: matchTier.exact, ranges: [[0, title.length]] }
  if (aliases.some((alias) => alias.toLowerCase() === needle)) {
    const start = wordStartIndex(text, needle)
    return { score: matchTier.alias, ranges: start >= 0 ? [[start, start + needle.length]] : [] }
  }
  // An alias prefix lets "acc" reach "Open Accounts" ahead of titles that merely start with it.
  const aliasPrefix = aliases.find((alias) => alias.toLowerCase().startsWith(needle))
  if (aliasPrefix) {
    const start = text.indexOf(aliasPrefix.toLowerCase())
    return {
      score: matchTier.aliasPrefix,
      ranges: start >= 0 ? [[start, start + needle.length]] : [],
    }
  }
  if (text.startsWith(needle)) return { score: matchTier.prefix, ranges: [[0, needle.length]] }
  const start = wordStartIndex(text, needle)
  if (start >= 0) return { score: matchTier.wordStart, ranges: [[start, start + needle.length]] }
  // A single character only means "a word starts with this letter" in the title.
  if (needle.length === 1) return noMatch

  const terms = needle.split(/\s+/)
  if (terms.length > 1) {
    const titleRanges: MatchRange[] = []
    for (const term of terms) {
      const index = wordStartIndex(text, term)
      if (index < 0) break
      titleRanges.push([index, index + term.length])
    }
    if (titleRanges.length === terms.length)
      return { score: matchTier.allTermsInTitle, ranges: titleRanges }
  } else {
    const ranges = acronymRanges(text, needle)
    if (ranges) return { score: matchTier.acronym, ranges }
  }

  const haystack = [text, ...keywords.map((keyword) => keyword.toLowerCase())].join(' ')
  if (terms.every((term) => wordStartIndex(haystack, term) >= 0))
    return { score: matchTier.keyword, ranges: [] }
  if (needle.length < 3) return noMatch
  const substring = text.indexOf(needle)
  if (substring >= 0)
    return { score: matchTier.substring, ranges: [[substring, substring + needle.length]] }
  if (terms.every((term) => haystack.includes(term)))
    return { score: matchTier.keywordSubstring, ranges: [] }
  return noMatch
}

/** Recency-weighted use count; a command used twice today beats one used five times last month. */
export function frecency(usage: { count: number; lastUsed: number } | undefined, now = Date.now()) {
  if (!usage) return 0
  const ageDays = Math.max(0, now - usage.lastUsed) / dayMs
  return usage.count / (1 + ageDays / 7)
}

/** Ranking lift from general use; capped below the gap between neighbouring match tiers. */
export function frecencyBonus(value: number) {
  return Math.min(15, 6 * Math.log2(1 + value))
}

/** Ranking lift from having picked this result for the same query before. */
export function queryBonus(picks: number) {
  return Math.min(25, 12 * Math.log2(1 + picks))
}

const maxStoredQueries = 200

type QueryUsage = Record<string, Record<string, { count: number; lastUsed: number }>>

const isRecord = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value)

const normalizeQuery = (query: string) => query.trim().toLowerCase().slice(0, 40)

/** Which result was picked for each query, as an optional local ranking hint. */
const queryUsage = preference<QueryUsage>(
  'brief:palette-queries',
  (raw) => {
    try {
      const parsed: unknown = JSON.parse(raw ?? '{}')
      if (!isRecord(parsed)) return {}
      const usage: QueryUsage = {}
      for (const [query, ids] of Object.entries(parsed)) {
        if (!isRecord(ids)) continue
        const picks: QueryUsage[string] = {}
        for (const [id, pick] of Object.entries(ids)) {
          if (isRecord(pick) && typeof pick.count === 'number' && typeof pick.lastUsed === 'number')
            picks[id] = { count: pick.count, lastUsed: pick.lastUsed }
        }
        usage[query] = picks
      }
      return usage
    } catch {
      return {}
    }
  },
  JSON.stringify,
)

export const getQueryUsage = queryUsage.get

/** Remembers which result was picked for a query, so typing it again ranks that result first. */
export function recordQueryPick(query: string, id: string, now = Date.now()) {
  const key = normalizeQuery(query)
  if (!key) return
  const usage = getQueryUsage()
  const picks = {
    ...usage[key],
    [id]: { count: (usage[key]?.[id]?.count ?? 0) + 1, lastUsed: now },
  }
  const kept = Object.entries({ ...usage, [key]: picks })
    .toSorted(
      ([, left], [, right]) =>
        Math.max(...Object.values(right).map(({ lastUsed }) => lastUsed)) -
        Math.max(...Object.values(left).map(({ lastUsed }) => lastUsed)),
    )
    .slice(0, maxStoredQueries)
  queryUsage.save(Object.fromEntries(kept))
}

/** Picks for this id under any remembered query that the current query is a prefix of. */
export function queryPicks(usage: QueryUsage, query: string, id: string, now = Date.now()) {
  const key = normalizeQuery(query)
  if (!key) return 0
  let picks = 0
  for (const [stored, ids] of Object.entries(usage)) {
    if (stored.startsWith(key) && ids[id]) picks += frecency(ids[id], now)
  }
  return picks
}

/** Drops every remembered query pick for one result, for "Reset ranking". */
export function forgetQueryPicks(id: string) {
  const kept = Object.entries(getQueryUsage()).flatMap(([query, { [id]: _forgotten, ...ids }]) =>
    Object.keys(ids).length ? [[query, ids] as const] : [],
  )
  queryUsage.save(Object.fromEntries(kept))
}

/** Favorite result ids in the order the user arranged them. */
const favorites = preference(
  'brief:palette-favorites',
  (raw): string[] => {
    try {
      const parsed: unknown = JSON.parse(raw ?? '[]')
      return Array.isArray(parsed)
        ? parsed.filter((id): id is string => typeof id === 'string')
        : []
    } catch {
      return []
    }
  },
  (ids: readonly string[]) => JSON.stringify([...new Set(ids)]),
)
export const getPaletteFavorites = favorites.get
export const savePaletteFavorites = favorites.save

export type PaletteMode = 'search' | 'ask'
/** The mode the palette last opened in. */
export const paletteMode = preference<PaletteMode>('brief:palette-mode', (raw) =>
  raw === 'ask' ? 'ask' : 'search',
)

export function toggleFavorite(ids: readonly string[], id: string) {
  return ids.includes(id) ? ids.filter((value) => value !== id) : [...ids, id]
}

export function moveFavorite(ids: readonly string[], id: string, offset: -1 | 1) {
  const index = ids.indexOf(id)
  const target = index + offset
  if (index < 0 || target < 0 || target >= ids.length) return [...ids]
  const next = [...ids]
  ;[next[index], next[target]] = [next[target], next[index]]
  return next
}

export type PaletteTab = {
  id: 'all' | 'commands' | 'accounts' | 'holdings' | 'activity'
  label: string
  /** Result groups the tab shows; `undefined` means every group. */
  groups?: readonly string[]
  /** Typing this first jumps to the tab, like VS Code Quick Open prefixes. */
  prefix?: string
}

// Type tabs follow claude.ai's palette: visible, and changed with ← and →.
export const paletteTabs: readonly PaletteTab[] = [
  { id: 'all', label: 'All' },
  {
    id: 'commands',
    label: 'Commands',
    groups: ['Here', 'View', 'Navigate', 'Analytics', 'Appearance', 'Actions'],
    prefix: '>',
  },
  { id: 'accounts', label: 'Accounts', groups: ['Accounts'], prefix: '@' },
  { id: 'holdings', label: 'Holdings', groups: ['Holdings'] },
  { id: 'activity', label: 'Activity', groups: ['Activity'] },
]

/** The tab a leading prefix asks for, and the query without it. */
export function paletteTabPrefix(query: string) {
  const trimmed = query.trimStart()
  const tab = paletteTabs.find(({ prefix }) => prefix && trimmed.startsWith(prefix))
  return tab ? { tab, term: trimmed.slice(1).trimStart() } : undefined
}
