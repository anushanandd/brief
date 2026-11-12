import { z } from 'zod'

import type { AccountGroup } from './account-groups'
import { accountDisplayName, type AccountDisplayNames } from './account-names'
import type { AppAction } from './action-system'
import { preference } from './preference'
import type { Account } from './schema'

/**
 * The number keys, 1 through 9 then 0, are the user's bookmarks. A key does nothing until the
 * user assigns it, in Settings › Navigation, to a page or report, an account, a type of account,
 * or a held security. A page or report opens by running its navigation action, whose first
 * binding the key becomes; the rest get an action of their own. Either way the palette, the key
 * guide, and ⌘/ show the key as they show any other.
 */
export const numberKeys = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'] as const
export type NumberKey = (typeof numberKeys)[number]

/** A page or report a number key may open, by its navigation action. */
export const isDestination = (id: string) =>
  id.startsWith('navigation.open.') || id.startsWith('navigation.report.')

const savedName = z.string().max(200).optional()
/**
 * What a key opens, as saved. Each carries the name it went by when chosen, so a destination
 * that has since gone (a closed account, a sold security) can still be named in Settings. A
 * security is saved as its ticker, never its place in a list.
 */
const savedTarget = z.discriminatedUnion('kind', [
  z.object({ kind: z.literal('page'), action: z.string().refine(isDestination), name: savedName }),
  z.object({ kind: z.literal('account'), account: z.string().min(1), name: savedName }),
  z.object({ kind: z.literal('group'), group: z.string().min(1), name: savedName }),
  z.object({ kind: z.literal('ticker'), ticker: z.string().min(1).max(32), name: savedName }),
])
const savedKeys = z.record(z.string(), z.unknown())
export type NumberKeyTarget = z.infer<typeof savedTarget>

/** Each assigned key's target; a key left out is unassigned. */
export type NumberKeyChoices = Partial<Record<NumberKey, NumberKeyTarget>>

/**
 * The saved choices, keyed by digit. Anything else reads as unassigned, key by key: the
 * first version's list of action IDs, one per key, held the sidebar's defaults alongside any
 * real choice, so it is dropped whole rather than turned into bookmarks nobody made.
 */
export function parseNumberKeys(raw: string | null): NumberKeyChoices {
  let value: unknown
  try {
    value = raw ? JSON.parse(raw) : null
  } catch {
    return {}
  }
  const saved = Array.isArray(value) ? undefined : savedKeys.safeParse(value).data
  if (!saved) return {}
  return Object.fromEntries(
    numberKeys.flatMap((key) => {
      const parsed = savedTarget.safeParse(saved[key])
      return parsed.success ? [[key, parsed.data]] : []
    }),
  )
}

export const numberKeyPreference = preference('brief.numberKeys', parseNumberKeys, JSON.stringify)

/** The same choices with `key` opening `target`, or nothing. */
export function assignNumberKey(
  choices: NumberKeyChoices,
  key: NumberKey,
  target: NumberKeyTarget | null,
): NumberKeyChoices {
  const { [key]: _replaced, ...rest } = choices
  return target ? { ...rest, [key]: target } : rest
}

/** One identity per destination, whatever name it was saved under. */
export const targetValue = (target: NumberKeyTarget) =>
  target.kind === 'page'
    ? `page:${target.action}`
    : target.kind === 'account'
      ? `account:${target.account}`
      : target.kind === 'group'
        ? `group:${target.group}`
        : `ticker:${target.ticker}`

/** Where an account or a type of account opens on Accounts, as its search. */
export type AccountsSearch = { group: string } | { account: string }

/** Something a number key can open now, as Settings lists it. */
export type NumberKeyDestination = {
  target: NumberKeyTarget
  value: string
  /** How Settings lists it. */
  label: string
  /** Its action's title, for one that has no action of its own. */
  title: string
  section: 'Pages' | 'Reports' | 'Accounts' | 'Holdings'
  /** Accounts' search, for an account or a type of account. */
  search?: AccountsSearch
}

/**
 * Every destination there is now, in Settings' order: pages and reports by their navigation
 * actions, then each type of account and its accounts in the Accounts menu's order, then each
 * security held, once however many accounts hold it.
 */
export function numberKeyDestinations({
  actions,
  groups,
  names,
  holdings,
}: {
  actions: readonly AppAction[]
  groups: readonly (Pick<AccountGroup, 'type' | 'label'> & {
    accounts: readonly Pick<Account, 'id' | 'name' | 'institution'>[]
  })[]
  names: AccountDisplayNames
  holdings: readonly { ticker: string; name?: string | null }[]
}): NumberKeyDestination[] {
  const destination = (
    target: NumberKeyTarget,
    entry: Omit<NumberKeyDestination, 'target' | 'value'>,
  ): NumberKeyDestination => ({ target, value: targetValue(target), ...entry })
  const pages = actions.filter(({ id }) => isDestination(id))
  const page = (action: AppAction, section: 'Pages' | 'Reports') => {
    const label = action.title.replace(/^Open /, '')
    return destination(
      { kind: 'page', action: action.id, name: label },
      { label, title: action.title, section },
    )
  }
  return [
    ...pages.filter(({ id }) => id.startsWith('navigation.open.')).map((a) => page(a, 'Pages')),
    ...pages.filter(({ id }) => id.startsWith('navigation.report.')).map((a) => page(a, 'Reports')),
    ...groups.flatMap((group) => {
      const [only, ...more] = group.accounts
      const label = `All ${group.label.toLowerCase()}`
      return [
        ...(only
          ? [
              destination(
                { kind: 'group', group: group.type, name: label },
                {
                  label,
                  title: `Open all ${group.label.toLowerCase()}`,
                  section: 'Accounts',
                  // A type with one account has no total of its own; it opens that account.
                  search: more.length ? { group: group.type } : { account: only.id },
                },
              ),
            ]
          : []),
        ...group.accounts.map((account) => {
          const shown = accountDisplayName(account.id, account.name, names)
          return destination(
            { kind: 'account', account: account.id, name: shown },
            {
              label: account.institution ? `${shown} · ${account.institution}` : shown,
              title: `Open ${shown}`,
              section: 'Accounts',
              search: { account: account.id },
            },
          )
        }),
      ]
    }),
    ...holdings.map(({ ticker, name }) =>
      destination(
        { kind: 'ticker', ticker, name: name || ticker },
        {
          label: name ? `${ticker} · ${name}` : ticker,
          title: `Open ${ticker}`,
          section: 'Holdings',
        },
      ),
    ),
  ]
}

/** What each assigned key opens now; a key whose destination is gone opens nothing. */
export function resolveNumberKeys(
  choices: NumberKeyChoices,
  destinations: readonly NumberKeyDestination[],
): Partial<Record<NumberKey, NumberKeyDestination>> {
  const byValue = new Map(destinations.map((destination) => [destination.value, destination]))
  return Object.fromEntries(
    numberKeys.flatMap((key) => {
      const target = choices[key]
      const destination = target && byValue.get(targetValue(target))
      return destination ? [[key, destination]] : []
    }),
  )
}

/** Each key assigned to a page or report as that action's first binding. */
export function withNumberKeys(
  actions: readonly AppAction[],
  resolved: Partial<Record<NumberKey, NumberKeyDestination>>,
): AppAction[] {
  return actions.map((action) => {
    const keys = numberKeys.filter((key) => {
      const target = resolved[key]?.target
      return target?.kind === 'page' && target.action === action.id
    })
    if (!keys.length) return action
    return {
      ...action,
      bindings: [
        ...keys.map((key) => ({ sequence: [key], label: key })),
        ...(action.bindings ?? []),
      ],
    }
  })
}
