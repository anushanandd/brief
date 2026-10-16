import type { ReactNode } from 'react'

import type { IconComponent } from '../components/icons'
import type { AppAction } from './action-system'
import { getActionUsage } from './action-system'
import { filterFields, type FilterStep } from './palette-filters'
import {
  frecency,
  frecencyBonus,
  getQueryUsage,
  matchPaletteItem,
  queryBonus,
  queryPicks,
  type MatchRange,
  type PaletteTab,
} from './palette-ranking'
import type { FinanceSnapshot } from './schema'

/** A follow-up for a row, run from the Actions menu or with its shortcut. */
export type RowAction = {
  label: string
  icon: IconComponent
  /** Keeps the palette open and confirms in the footer, for in-place actions like copying. */
  notice?: string
  run: () => unknown
}

/** One palette row: a registered action, or a page, account, holding, or activity entry. */
export type MenuAction = {
  id: string
  label: string
  search: string
  group: string
  icon: IconComponent
  detail?: string
  shortcut?: string
  disabled?: boolean
  priority?: number
  /** What Enter does, named first in the Actions menu; defaults by group. */
  verb?: string
  /** A row's own identity mark, such as an account type or ticker; defaults to a toned icon. */
  mark?: ReactNode
  /** Exact-match names, such as a ticker, that rank like the title itself. */
  aliases?: readonly string[]
  /** Score for rows already derived from the query, such as Activity search. */
  fixedScore?: number
  /** Rows built from the query itself cannot be favorited because they do not persist. */
  ephemeral?: boolean
  /** Filter choices change the palette instead of leaving it. */
  keepsOpen?: boolean
  /** Run with ⌘↵. */
  secondary?: RowAction
  /** Further rows in the Actions menu. */
  extras?: readonly RowAction[]
  registeredAction?: AppAction
  run: () => unknown
}

export type RankedAction = MenuAction & { ranges?: readonly MatchRange[] }

/** One line of the Actions menu beside a row. */
export type MenuRow = RowAction & { id: string; disabled?: boolean; separated?: boolean }

export const commandGroups: { group: string; heading: string }[] = [
  { group: 'Here', heading: 'This page' },
  { group: 'View', heading: 'View' },
  { group: 'Navigate', heading: 'Navigate' },
  { group: 'Analytics', heading: 'Charts' },
  { group: 'Appearance', heading: 'Appearance' },
  { group: 'Actions', heading: 'Actions' },
]

// The empty palette offers a few everyday actions after Favorites and Recents, as claude.ai does.
const everydayActions = [
  'finance.refresh',
  'refresh',
  'privacy.toggle-values',
  'settings.open',
  'shortcuts.open',
  'shortcuts',
]

export const groupVerb: Record<string, string> = {
  Navigate: 'Open page',
  Analytics: 'Open chart',
  Accounts: 'Open account',
  Holdings: 'Open holding',
  Activity: 'Show entry',
}

export const groupKind: Record<string, string> = {
  Here: 'This page',
  View: 'This page',
  Navigate: 'Page',
  Analytics: 'Chart',
  Accounts: 'Account',
  Holdings: 'Holding',
  Activity: 'Activity',
}

// Each kind of result keeps one tone, drawn from the shared data tokens, so rows scan by color.
export const groupTone: Record<string, string> = {
  Navigate: 'accent',
  Analytics: 'data-3',
  Activity: 'data-6',
  Accounts: 'data-4',
  Holdings: 'data-1',
}

const maxResults = 40
const favoriteBoost = 10

/** Each held ticker once, with its name. */
export function heldSecurities(
  holdings: readonly Pick<FinanceSnapshot['holdings'][number], 'ticker' | 'name'>[],
) {
  return [...new Map(holdings.map(({ ticker, name }) => [ticker, name])).entries()].map(
    ([ticker, name]) => ({ ticker, name }),
  )
}

/** The question after a leading `?`, or `null` for a search. */
export function paletteQuestion(query: string) {
  const trimmed = query.trim()
  return trimmed.startsWith('?') ? trimmed.slice(1).trim() : null
}

/** What ranking reads besides the rows: the query and how results have been used. */
export type RankingContext = {
  query: string
  usage: ReturnType<typeof getActionUsage>
  queryUsage: ReturnType<typeof getQueryUsage>
  favorites: ReadonlySet<string>
  now: number
}

const usageBonus = (action: MenuAction, ranking: RankingContext) =>
  frecencyBonus(frecency(ranking.usage[action.id], ranking.now)) +
  (ranking.favorites.has(action.id) ? favoriteBoost : 0)

/** One ranking decides the order: match tier, then use for this query, then general use. */
export function rankMenuActions(
  candidates: readonly MenuAction[],
  ranking: RankingContext,
): RankedAction[] {
  return candidates
    .flatMap((action) => {
      const match =
        action.fixedScore === undefined
          ? matchPaletteItem(ranking.query, action.label, {
              keywords: [action.search],
              aliases: action.aliases,
            })
          : { score: action.fixedScore, ranges: [] }
      if (!match.score) return []
      const score =
        match.score +
        ((action.priority ?? 1) - 1) * 3 +
        usageBonus(action, ranking) +
        queryBonus(queryPicks(ranking.queryUsage, ranking.query, action.id, ranking.now)) -
        (action.disabled ? 30 : 0)
      return [{ action: { ...action, ranges: match.ranges }, score }]
    })
    .toSorted((left, right) => right.score - left.score)
    .slice(0, maxResults)
    .map(({ action }) => action)
}

export type PaletteSection = { heading?: string; items: readonly RankedAction[] }

/** What the palette lists in each state: the picker, a search, or the empty palette's groups. */
export function paletteSections(input: {
  askMode: boolean
  filterStep: FilterStep | undefined
  filterRows: readonly MenuAction[]
  tab: PaletteTab
  chipCount: number
  actions: readonly MenuAction[]
  favorites: readonly string[]
  recentActivity: readonly MenuAction[]
  ranking: RankingContext
}): PaletteSection[] {
  const { askMode, filterStep, filterRows, tab, chipCount, actions, favorites, ranking } = input
  if (askMode) return []
  const rank = (candidates: readonly MenuAction[]) => rankMenuActions(candidates, ranking)
  if (filterStep)
    return [
      {
        heading:
          filterStep === 'field'
            ? undefined
            : filterFields.find(({ field }) => field === filterStep)?.label,
        items: ranking.query ? rank(filterRows) : filterRows,
      },
    ]
  const candidates = actions.filter((action) => !tab.groups || tab.groups.includes(action.group))
  if (ranking.query) return [{ items: rank(candidates) }]
  if (tab.id === 'all' && !chipCount) {
    const byId = new Map(actions.map((action) => [action.id, action]))
    const favoriteRows = favorites.flatMap((id) => {
      const action = byId.get(id)
      return action ? [action] : []
    })
    const recent = actions
      .filter(
        (action) =>
          !action.disabled && ranking.usage[action.id] && !ranking.favorites.has(action.id),
      )
      .toSorted(
        (left, right) =>
          frecency(ranking.usage[right.id], ranking.now) -
          frecency(ranking.usage[left.id], ranking.now),
      )
      .slice(0, 5)
    const shown = new Set([...favoriteRows, ...recent].map(({ id }) => id))
    const everyday = everydayActions.flatMap((id) => {
      const action = byId.get(id)
      return action && !shown.has(id) ? [action] : []
    })
    return [
      { heading: 'Favorites', items: favoriteRows },
      { heading: 'Recents', items: recent },
      { heading: 'Actions', items: everyday },
    ]
  }
  if (tab.id === 'commands')
    return commandGroups.map(({ group, heading }) => ({
      heading,
      items: candidates
        .filter((action) => action.group === group)
        .toSorted((left, right) => (right.priority ?? 0) - (left.priority ?? 0)),
    }))
  if (tab.id === 'activity' && !chipCount)
    return [{ heading: 'Recent activity', items: input.recentActivity }]
  return [{ items: candidates }]
}

/** What the Actions menu offers for a row: its verb, follow-ups, favorites, and a reset. */
export function menuRows(
  target: MenuAction,
  favorites: readonly string[],
  on: {
    run: (target: MenuAction) => void
    runFollowUp: (target: MenuAction, followUp: RowAction) => void
    toggleFavorite: (target: MenuAction) => void
    moveFavorite: (target: MenuAction, offset: -1 | 1) => void
    resetRanking: (target: MenuAction) => void
  },
  icons: { star: IconComponent; reset: IconComponent },
): MenuRow[] {
  const isFavorite = favorites.includes(target.id)
  const index = favorites.indexOf(target.id)
  return [
    {
      id: 'primary',
      label: target.verb ?? groupVerb[target.group] ?? 'Run',
      icon: target.icon,
      disabled: target.disabled,
      run: () => on.run(target),
    },
    ...(target.secondary
      ? [
          {
            id: 'secondary',
            ...target.secondary,
            run: () => on.runFollowUp(target, target.secondary!),
          },
        ]
      : []),
    ...(target.extras ?? []).map((extra, extraIndex) => ({
      id: `extra:${extraIndex}`,
      ...extra,
      run: () => on.runFollowUp(target, extra),
    })),
    ...(target.ephemeral
      ? []
      : [
          {
            id: 'favorite',
            label: isFavorite ? 'Remove from Favorites' : 'Add to Favorites',
            icon: icons.star,
            run: () => on.toggleFavorite(target),
          },
          ...(isFavorite && index > 0
            ? [
                {
                  id: 'favorite-up',
                  label: 'Move Favorite up',
                  icon: icons.star,
                  run: () => on.moveFavorite(target, -1),
                },
              ]
            : []),
          ...(isFavorite && index < favorites.length - 1
            ? [
                {
                  id: 'favorite-down',
                  label: 'Move Favorite down',
                  icon: icons.star,
                  run: () => on.moveFavorite(target, 1),
                },
              ]
            : []),
          {
            id: 'reset',
            label: 'Reset ranking',
            icon: icons.reset,
            separated: true,
            run: () => on.resetRanking(target),
          },
        ]),
  ]
}
