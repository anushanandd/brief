/**
 * Reads an Activity filter the way it would be said: "dining over $50 last month on platinum".
 * Known accounts, categories, and methods become filters, amount and date phrases become
 * bounds, and whatever is left is ordinary text search. Pure, so typing previews instantly.
 */

import { dayKey, dayMs, dayStart, isCalendarDay } from './dates'

export type QueryVocabulary = {
  accounts: ReadonlyArray<{ id: string; label: string }>
  categories: readonly string[]
  methods: readonly string[]
}

export type ParsedActivityQuery = {
  accounts: string[]
  categories: string[]
  methods: string[]
  from?: string
  to?: string
  min?: number
  max?: number
  /** Whose money: yours, or someone else's. */
  owner?: 'me' | 'other'
  text: string
}

const monthNames = [
  'january',
  'february',
  'march',
  'april',
  'may',
  'june',
  'july',
  'august',
  'september',
  'october',
  'november',
  'december',
]
const fillers = new Set([
  'on',
  'at',
  'in',
  'from',
  'for',
  'with',
  'the',
  'and',
  'my',
  'spent',
  'spending',
  'transactions',
  'purchases',
  'payments',
  'activity',
])
const escape = (value: string) => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
const amount = String.raw`\$?(\d[\d,]*(?:\.\d+)?)`
const money = (value: string) => Number(value.replaceAll(',', ''))

function monthRange(year: number, month: number) {
  return {
    from: dayKey(Date.UTC(year, month, 1)),
    to: dayKey(Date.UTC(year, month + 1, 0)),
  }
}

/** Date phrases, most specific first. Each returns inclusive bounds ending no later than today. */
function datePhrases(today: string) {
  const now = new Date(dayStart(today))
  const year = now.getUTCFullYear()
  const month = now.getUTCMonth()
  const weekday = (now.getUTCDay() + 6) % 7
  const days = (count: number) => ({
    from: dayKey(dayStart(today) - (count - 1) * dayMs),
    to: today,
  })
  const phrases: Array<[RegExp, () => { from: string; to: string }]> = [
    [/\btoday\b/, () => ({ from: today, to: today })],
    [
      /\byesterday\b/,
      () => ({ from: dayKey(dayStart(today) - dayMs), to: dayKey(dayStart(today) - dayMs) }),
    ],
    [/\bpast week\b/, () => days(7)],
    [/\bthis week\b/, () => ({ from: dayKey(dayStart(today) - weekday * dayMs), to: today })],
    [
      /\blast week\b/,
      () => ({
        from: dayKey(dayStart(today) - (weekday + 7) * dayMs),
        to: dayKey(dayStart(today) - (weekday + 1) * dayMs),
      }),
    ],
    [/\bthis month\b/, () => ({ from: dayKey(Date.UTC(year, month, 1)), to: today })],
    [/\blast month\b/, () => monthRange(year, month - 1)],
    [/\bthis year\b/, () => ({ from: `${year}-01-01`, to: today })],
    [/\blast year\b/, () => ({ from: `${year - 1}-01-01`, to: `${year - 1}-12-31` })],
  ]
  return phrases
}

// "aug", "august", "sept", and "september" all name the same month.
const monthPattern = String.raw`(jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|june?|july?|aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)`
const dayPattern = String.raw`${monthPattern}(?: (\d{1,2})(?:st|nd|rd|th)?)?(?:,? (\d{4}))?`
const monthIndex = (name: string) =>
  monthNames.findIndex((month) => month.startsWith(name.slice(0, 3)))

/** A month (and optional day and year) as a date; the year defaults to the latest one passed. */
function namedDay(
  today: string,
  name: string,
  day: string | undefined,
  year: string | undefined,
  end: boolean,
) {
  const month = monthIndex(name)
  const current = new Date(dayStart(today))
  const resolvedYear = year
    ? Number(year)
    : current.getUTCFullYear() - (month > current.getUTCMonth() ? 1 : 0)
  if (day) return dayKey(Date.UTC(resolvedYear, month, Number(day)))
  return end ? monthRange(resolvedYear, month).to : monthRange(resolvedYear, month).from
}

function parseDates(text: string, today: string): { text: string; from?: string; to?: string } {
  const relative = /\b(?:last|past) (\d{1,3}) days\b/.exec(text)
  if (relative) {
    const count = Math.max(1, Number(relative[1]))
    return {
      text: text.replace(relative[0], ' '),
      from: dayKey(dayStart(today) - (count - 1) * dayMs),
      to: today,
    }
  }
  for (const [pattern, range] of datePhrases(today)) {
    const match = pattern.exec(text)
    if (match) return { text: text.replace(match[0], ' '), ...range() }
  }
  const span = new RegExp(
    String.raw`\b(?:from |between )?${dayPattern} (?:to|-|–|through|until|and) ${dayPattern}\b`,
  ).exec(text)
  if (span) {
    // A year written on one end carries to the other, wrapping when the months cross a new year.
    const wraps = monthIndex(span[4]) < monthIndex(span[1])
    const startYear = span[3] ?? (span[6] && (wraps ? String(Number(span[6]) - 1) : span[6]))
    const endYear = span[6] ?? (span[3] && (wraps ? String(Number(span[3]) + 1) : span[3]))
    const start = namedDay(today, span[1], span[2], startYear || undefined, false)
    const end = namedDay(today, span[4], span[5], endYear || undefined, true)
    return { text: text.replace(span[0], ' '), from: start, to: end < start ? start : end }
  }
  const since = new RegExp(String.raw`\b(since|after|before|until) ${dayPattern}\b`).exec(text)
  if (since) {
    const [, word, name, day, year] = since
    const date = namedDay(today, name, day, year, word === 'until')
    const rest = text.replace(since[0], ' ')
    if (word === 'since') return { text: rest, from: date, to: today }
    if (word === 'after') return { text: rest, from: dayKey(dayStart(date) + dayMs), to: today }
    if (word === 'before') return { text: rest, to: dayKey(dayStart(date) - dayMs) }
    return { text: rest, to: date }
  }
  const named = new RegExp(String.raw`\b(?:in )?${dayPattern}\b`).exec(text)
  if (named) {
    const [, name, day, year] = named
    const start = namedDay(today, name, day, year, false)
    return {
      text: text.replace(named[0], ' '),
      from: start,
      to: day ? start : namedDay(today, name, undefined, year, true),
    }
  }
  return { text }
}

/**
 * Rewrites written dates the date phrases can read: "8/1", "8/1/26", and "2026-08-01" become
 * month names, and "aug 1 - 15" carries its month to the second day. Invalid days such as
 * 2/31 are left alone, so they stay search text.
 */
function normalizeDates(text: string) {
  const name = (month: number) => monthNames[month - 1]?.slice(0, 3)
  return text
    .replace(/(?<=\s)(\d{4})-(\d{1,2})-(\d{1,2})(?=\s)/g, (match, y, m, d) =>
      isCalendarDay(Number(y), Number(m), Number(d))
        ? `${name(Number(m))} ${Number(d)} ${y}`
        : match,
    )
    .replace(
      /(?<=\s)(\d{1,2})\/(\d{1,2})(?:\/(\d{2}|\d{4}))?(?=\s)/g,
      (match, m, d, y: string | undefined) => {
        const year = y ? Number(y.length === 2 ? `20${y}` : y) : 2000
        return isCalendarDay(year, Number(m), Number(d))
          ? `${name(Number(m))} ${Number(d)}${y ? ` ${year}` : ''}`
          : match
      },
    )
    .replace(
      new RegExp(String.raw`${monthPattern} (\d{1,2}) ?(?:-|–|to|through) ?(\d{1,2})(?=\s)`, 'g'),
      (_, month: string, first: string, last: string) => `${month} ${first} to ${month} ${last}`,
    )
    .replace(/(\d) ?[-–] ?(?=[a-z])/g, '$1 to ')
}

function parseAmounts(text: string) {
  let rest = text
  let min: number | undefined
  let max: number | undefined
  const between = new RegExp(String.raw`\bbetween ${amount} and ${amount}`).exec(rest)
  const span = new RegExp(String.raw`\$(\d[\d,]*(?:\.\d+)?)\s*(?:-|to)\s*${amount}`).exec(rest)
  const range = between ?? span
  if (range) {
    min = Math.min(money(range[1]), money(range[2]))
    max = Math.max(money(range[1]), money(range[2]))
    rest = rest.replace(range[0], ' ')
  }
  const over = new RegExp(
    String.raw`(?:\b(?:over|above|more than|at least)\s*|>=?\s*)${amount}`,
  ).exec(rest)
  if (over) {
    min = money(over[1])
    rest = rest.replace(over[0], ' ')
  }
  const under = new RegExp(
    String.raw`(?:\b(?:under|below|less than|at most)\s*|<=?\s*)${amount}`,
  ).exec(rest)
  if (under) {
    max = money(under[1])
    rest = rest.replace(under[0], ' ')
  }
  return { text: rest, min, max }
}

type Entity = { kind: 'accounts' | 'categories' | 'methods'; value: string; label: string }

/** Full labels, plus single distinctive words that point at exactly one entity. */
function vocabularyPhrases(vocabulary: QueryVocabulary) {
  const entities: Entity[] = [
    ...vocabulary.accounts.map(({ id, label }) => ({
      kind: 'accounts' as const,
      value: id,
      label,
    })),
    ...vocabulary.categories.map((value) => ({ kind: 'categories' as const, value, label: value })),
    ...vocabulary.methods.map((value) => ({ kind: 'methods' as const, value, label: value })),
  ]
  const phrases = new Map<string, Entity | null>()
  for (const entity of entities) phrases.set(entity.label.toLocaleLowerCase(), entity)
  for (const entity of entities) {
    for (const word of entity.label.toLocaleLowerCase().split(/\s+/)) {
      if (word.length < 4 || fillers.has(word) || phrases.get(word) === entity) continue
      phrases.set(word, phrases.has(word) ? null : entity)
    }
  }
  return [...phrases]
    .filter((entry): entry is [string, Entity] => entry[1] !== null)
    .toSorted((a, b) => b[0].length - a[0].length)
}

export function parseActivityQuery(
  input: string,
  vocabulary: QueryVocabulary,
  today: string,
): ParsedActivityQuery {
  const result: ParsedActivityQuery = { accounts: [], categories: [], methods: [], text: '' }
  let text = ` ${input.toLocaleLowerCase().replace(/\s+/g, ' ')} `
  const amounts = parseAmounts(text)
  text = amounts.text
  if (amounts.min != null) result.min = amounts.min
  if (amounts.max != null) result.max = amounts.max
  const dates = parseDates(normalizeDates(text), today)
  text = dates.text
  if (dates.from) result.from = dates.from
  if (dates.to) result.to = dates.to
  // "for other" and "for me" before fillers drop the "for".
  const owner = /(^|\s)for (others?|me)(?=\s|$)/.exec(text)
  if (owner) {
    result.owner = owner[2] === 'me' ? 'me' : 'other'
    text = text.replace(owner[0], ' ')
  }
  for (const [phrase, entity] of vocabularyPhrases(vocabulary)) {
    const pattern = new RegExp(String.raw`(^|\s)${escape(phrase)}(?=\s|$)`)
    if (!pattern.test(text)) continue
    text = text.replace(pattern, ' ')
    if (!result[entity.kind].includes(entity.value)) result[entity.kind].push(entity.value)
  }
  result.text = text
    .split(' ')
    .filter((word) => word && !fillers.has(word))
    .join(' ')
  return result
}

export function hasStructuredQuery(query: ParsedActivityQuery) {
  return Boolean(
    query.accounts.length ||
    query.categories.length ||
    query.methods.length ||
    query.from ||
    query.to ||
    query.min != null ||
    query.max != null ||
    query.owner,
  )
}

export type QuerySuggestion = {
  id: string
  group: 'Accounts' | 'Categories' | 'Methods' | 'Dates' | 'Amounts' | 'For'
  label: string
  /** Text the suggestion stands for, appended to the committed query. */
  phrase: string
}

const datePresets = [
  'Today',
  'Yesterday',
  'This week',
  'Last week',
  'Last 30 days',
  'This month',
  'Last month',
  'This year',
]

/** Completions for the word being typed; the rest of the input is left as written. */
export function activityQuerySuggestions(input: string, vocabulary: QueryVocabulary, limit = 8) {
  const fragment = /(?:^|\s)([^\s]*)$/.exec(input)?.[1]?.toLocaleLowerCase() ?? ''
  const tail = /(?:^|\s)(\S+\s\S*)$/.exec(input)?.[1]?.toLocaleLowerCase() ?? fragment
  if (!fragment && !tail.trim()) return []
  const matches = (label: string) => {
    const value = label.toLocaleLowerCase()
    return (
      (fragment && value.split(/\s+/).some((word) => word.startsWith(fragment))) ||
      (tail.includes(' ') && value.startsWith(tail))
    )
  }
  const number = /^\$?(\d[\d,]*(?:\.\d+)?)$/.exec(fragment)
  const suggestions: QuerySuggestion[] = [
    ...(number
      ? [
          {
            id: 'amount:over',
            group: 'Amounts' as const,
            label: `Over $${number[1]}`,
            phrase: `over ${number[1]}`,
          },
          {
            id: 'amount:under',
            group: 'Amounts' as const,
            label: `Under $${number[1]}`,
            phrase: `under ${number[1]}`,
          },
        ]
      : []),
    ...vocabulary.categories.filter(matches).map((value) => ({
      id: `category:${value}`,
      group: 'Categories' as const,
      label: value,
      phrase: value,
    })),
    ...vocabulary.accounts
      .filter(({ label }) => matches(label))
      .map(({ id, label }) => ({
        id: `account:${id}`,
        group: 'Accounts' as const,
        label,
        phrase: label,
      })),
    ...vocabulary.methods.filter(matches).map((value) => ({
      id: `method:${value}`,
      group: 'Methods' as const,
      label: value,
      phrase: value,
    })),
    ...datePresets.filter(matches).map((value) => ({
      id: `date:${value}`,
      group: 'Dates' as const,
      label: value,
      phrase: value,
    })),
    ...['For other', 'For me'].filter(matches).map((value) => ({
      id: `for:${value}`,
      group: 'For' as const,
      label: value,
      phrase: value.toLocaleLowerCase(),
    })),
  ]
  // A suggestion that continues the last two words ("last mon" → Last month) ranks first.
  const continues = ({ label }: QuerySuggestion) =>
    tail.includes(' ') && label.toLocaleLowerCase().startsWith(tail)
  return suggestions.toSorted((a, b) => Number(continues(b)) - Number(continues(a))).slice(0, limit)
}

/** Replaces the word being typed (or a two-word prefix of the suggestion) with its phrase. */
export function completeActivityQuery(input: string, suggestion: QuerySuggestion) {
  const tail = /(?:^|\s)(\S+\s\S*)$/.exec(input)?.[1]
  const replaceTail =
    tail && suggestion.phrase.toLocaleLowerCase().startsWith(tail.toLocaleLowerCase())
  const kept = replaceTail
    ? input.slice(0, input.length - tail.length)
    : input.replace(/[^\s]*$/, '')
  return `${kept}${suggestion.phrase} `
}

export function amountMatches(value: number, min?: number, max?: number) {
  const size = Math.abs(value)
  return (min == null || size >= min) && (max == null || size <= max)
}

/** The draft without the fragment a suggestion is still completing, so previews stay useful. */
export function settledActivityQuery(input: string, suggestions: QuerySuggestion[]) {
  if (!suggestions.length || /\s$/.test(input)) return input
  const completed = completeActivityQuery(input, suggestions[0])
  return completed.slice(0, completed.length - suggestions[0].phrase.length - 1)
}
