import type { ContactFit } from './contacts'
import { daysBetween } from './dates'
import type { Settings } from './finance-settings'
import type { FinanceSnapshot, Person, Transaction, Trip } from './schema'

/**
 * A person's own color, one of the seven data colors, chosen from their id so it stays theirs
 * wherever they appear. It marks identity only, never gain or loss.
 */
export function personColor(id: string) {
  let hash = 0
  for (const character of id) hash = (hash * 31 + (character.codePointAt(0) ?? 0)) >>> 0
  return `var(--data-${(hash % 7) + 1})`
}

/** How long ago, in words: "today", "3 days ago", "2 months ago". */
export function timeAgo(date: string, today: string) {
  const days = Math.max(0, daysBetween(date, today))
  if (days === 0) return 'today'
  if (days === 1) return 'yesterday'
  if (days < 14) return `${days} days ago`
  if (days < 60) return `${Math.round(days / 7)} weeks ago`
  if (days < 365) return `${Math.round(days / 30.4)} months ago`
  const years = Math.floor(days / 365)
  return years === 1 ? 'a year ago' : `${years} years ago`
}

type Suggestion = NonNullable<FinanceSnapshot['insights']>['people']['suggestions'][number]

/** Someone a name in payments could be: a saved person, or a Contacts card not yet one. */
export type Identity =
  | { kind: 'person'; person: Person; fit: ContactFit['fit'] }
  | { kind: 'contact'; contact: ContactFit; fit: ContactFit['fit'] }

/**
 * Who a name in payments could be, likeliest first: the saved people Rust matched, then the
 * Contacts cards Rust matched. A card already linked to someone is that person, once.
 */
const fitRank = (item: Identity) => (item.fit === 'likely' ? 0 : 1)

export function identities(
  suggestion: Suggestion,
  people: readonly Person[],
  contacts: readonly ContactFit[] = [],
): Identity[] {
  const byId = new Map(people.map((person) => [person.id, person]))
  const byContact = new Map(
    people.flatMap((person) => (person.contactId ? [[person.contactId, person] as const] : [])),
  )
  const found: Identity[] = suggestion.matches.flatMap(({ id, fit }) => {
    const person = byId.get(id)
    return person ? [{ kind: 'person' as const, person, fit }] : []
  })
  for (const contact of contacts) {
    const person = byContact.get(contact.identifier)
    if (person) {
      if (!found.some((item) => item.kind === 'person' && item.person.id === person.id))
        found.push({ kind: 'person', person, fit: contact.fit })
    } else found.push({ kind: 'contact', contact, fit: contact.fit })
  }
  return found.toSorted((left, right) => fitRank(left) - fitRank(right))
}

/**
 * Settings once a name in payments is known to be someone: that person matches it from now
 * on, so every payment carrying it is theirs. A Contacts card becomes a person first.
 */
export function withIdentity(
  settings: Settings,
  matchName: string,
  identity: Identity | { kind: 'new'; name: string },
): Settings {
  if (identity.kind === 'person') {
    const names = identity.person.matchNames ?? []
    return withPerson(settings, {
      ...identity.person,
      matchNames: names.includes(matchName) ? names : [...names, matchName],
    })
  }
  return withPerson(
    settings,
    identity.kind === 'contact'
      ? newPerson(identity.contact.name, identity.contact.identifier, matchName)
      : newPerson(identity.name, undefined, matchName),
  )
}

/** Tagged transactions in each of the last twelve months, oldest first. */
export function monthlyMoments(dates: readonly string[], today: string) {
  const [year, month] = today.split('-').map(Number)
  const months = Array.from({ length: 12 }, (_, index) => {
    const date = new Date(Date.UTC(year, month - 12 + index, 1))
    return {
      key: date.toISOString().slice(0, 7),
      label: date.toLocaleString('en-US', { month: 'short', timeZone: 'UTC' }),
      count: 0,
    }
  })
  const byKey = new Map(months.map((item) => [item.key, item]))
  for (const date of dates) {
    const item = byKey.get(date.slice(0, 7))
    if (item) item.count += 1
  }
  return months
}

/** People by id, from the saved settings. */
export function peopleById(settings: Pick<Settings, 'people'>) {
  return new Map(settings.people.map((person) => [person.id, person]))
}

/** Up to two initials, for a person without a photo. */
export function initials(name: string) {
  const words = name.trim().split(/\s+/).filter(Boolean)
  const letters = words.length > 1 ? [words[0], words.at(-1) ?? ''] : words
  const segments = new Intl.Segmenter()
  return letters
    .map((word) => segments.segment(word)[Symbol.iterator]().next().value?.segment ?? '')
    .join('')
    .toUpperCase()
}

/** A new person's id; ids only need to be unique among the saved people. */
export function newPersonId() {
  return `person:${crypto.randomUUID()}`
}

/**
 * A person as first added: from a Contacts card or a typed name, matching payments by that
 * name unless it's a single word, which would match too much.
 */
export function newPerson(name: string, contactId?: string, matchName?: string): Person {
  const trimmed = name.trim()
  const match = matchName ?? (trimmed.includes(' ') ? trimmed : undefined)
  return {
    id: newPersonId(),
    name: trimmed,
    ...(contactId ? { contactId } : {}),
    ...(match ? { matchNames: [match] } : {}),
  }
}

/** People whose name contains every word typed, best first: a name starting with it first. */
export function filterPeople(people: readonly Person[], query: string) {
  const words = query.trim().toLowerCase().split(/\s+/).filter(Boolean)
  if (!words.length) return people.toSorted((left, right) => left.name.localeCompare(right.name))
  return people
    .filter((person) => words.every((word) => person.name.toLowerCase().includes(word)))
    .toSorted(
      (left, right) =>
        Number(right.name.toLowerCase().startsWith(words[0])) -
          Number(left.name.toLowerCase().startsWith(words[0])) ||
        left.name.localeCompare(right.name),
    )
}

/** The person a search names, when they exist. */
export function personFromSearch(data: FinanceSnapshot, id: string | undefined) {
  return id ? data.settings?.people?.find((person) => person.id === id) : undefined
}

/** A person's tagged transactions, newest first. */
export function personTransactions(data: FinanceSnapshot, id: string): Transaction[] {
  return data.transactions
    .filter((transaction) => transaction.people?.includes(id))
    .toSorted((left, right) =>
      (right.postedOn ?? right.date).localeCompare(left.postedOn ?? left.date),
    )
}

/**
 * Settings with a trip's companions replaced: every saved entry sharing a transaction with the
 * trip gives way to one naming its transactions now.
 */
export function withTripCompanions(
  settings: Settings,
  trip: Pick<Trip, 'transactionIds'>,
  people: string[],
): Settings {
  const ids = new Set(trip.transactionIds)
  const others = settings.tripCompanions.filter(
    (entry) => !entry.transactionIds.some((id) => ids.has(id)),
  )
  return {
    ...settings,
    tripCompanions: people.length
      ? [...others, { transactionIds: [...trip.transactionIds], people }]
      : others,
  }
}

/** Settings without a person, and without them on any trip. Their tags simply stop showing. */
export function withoutPerson(settings: Settings, id: string): Settings {
  return {
    ...settings,
    people: settings.people.filter((person) => person.id !== id),
    tripCompanions: settings.tripCompanions
      .map((entry) => ({ ...entry, people: entry.people.filter((person) => person !== id) }))
      .filter((entry) => entry.people.length),
  }
}

/** Settings with one person changed in place. */
export function withPerson(settings: Settings, person: Person): Settings {
  const known = settings.people.some(({ id }) => id === person.id)
  return {
    ...settings,
    people: known
      ? settings.people.map((current) => (current.id === person.id ? person : current))
      : [...settings.people, person],
  }
}
