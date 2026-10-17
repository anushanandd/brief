import type { useNavigate } from '@tanstack/react-router'
import type { ReactNode } from 'react'

import type { AppAction } from '../lib/action-system'
import { actionScopePriority } from '../lib/action-system'
import { activityMethod, type ActivityItem } from '../lib/activity'
import { localDateKey, validDate } from '../lib/dates'
import { formatCurrency, pluralize } from '../lib/format'
import { stockLogoUrl, stockMarkColor, stockMarkLabel } from '../lib/logos'
import { analyticsNavigation, secondaryNavigation } from '../lib/navigation'
import type { useOrderedNavigation } from '../lib/navigation-preferences'
import {
  activityObjectId,
  getObject,
  objectKinds,
  openObject,
  type ObjectKind,
  type SearchHit,
} from '../lib/objects'
import {
  chipSearch,
  datePresets,
  filterFields,
  type FilterChip,
  type FilterField,
  type FilterStep,
} from '../lib/palette-filters'
import type { MatchRange } from '../lib/palette-ranking'
import { groupTone, heldSecurities, type MenuAction, type RowAction } from '../lib/palette-rows'
import type { FinanceSettings, FinanceSnapshot } from '../lib/schema'
import { settingsHash } from '../lib/settings-dialog'
import { transactionDateKey } from '../lib/spending'
import { objectVerbs, verbs, type Verb } from '../lib/verbs'
import { AccountMark } from './account-mark'
import { ActivityMark } from './activity-list'
import { ActivityMethodMark } from './activity-method-mark'
import { BrandMark } from './brand-mark'
import { CategoryMark } from './category-mark'
import {
  Activity,
  ArrowLeftRight,
  BriefcaseBusiness,
  Building,
  Copy,
  CreditCard,
  Database,
  FileText,
  Gift,
  HelpCircle,
  Info,
  Landmark,
  List,
  MapPinned,
  Plane,
  Receipt,
  ReceiptText,
  RefreshCw,
  ScrollText,
  Search,
  ShieldCheck,
  TrendingUp,
  Users,
  type IconComponent,
} from './icons'
import { Keys } from './keys'

export function RowMark({ action }: { action: Pick<MenuAction, 'mark' | 'icon' | 'group'> }) {
  if (action.mark) return action.mark
  const Icon = action.icon
  return (
    <span className={`command-mark command-mark-${groupTone[action.group] ?? 'neutral'}`}>
      <Icon size={14} aria-hidden="true" />
    </span>
  )
}

// The same company mark as Holdings: a logo when external logos are on, the ticker circle otherwise.
const tickerMark = (ticker: string, externalLogos: boolean) => (
  <BrandMark
    className="asset-mark"
    fallback={stockMarkLabel(ticker)}
    label=""
    src={stockLogoUrl(ticker, externalLogos)}
    style={{ backgroundColor: stockMarkColor(ticker) }}
  />
)

export function Highlighted({
  text,
  ranges = [],
}: {
  text: string
  ranges?: readonly MatchRange[]
}) {
  if (!ranges.length) return text
  const parts: ReactNode[] = []
  let cursor = 0
  for (const [start, end] of ranges.toSorted((left, right) => left[0] - right[0])) {
    if (start < cursor) continue
    if (start > cursor) parts.push(text.slice(cursor, start))
    parts.push(<mark key={start}>{text.slice(start, end)}</mark>)
    cursor = end
  }
  parts.push(text.slice(cursor))
  return parts
}

export function FooterHint({ label, keys }: { label: string; keys: readonly string[] }) {
  return (
    <span className="command-footer-hint">
      {label}
      {keys.map((key) => (
        <Keys label={key} key={key} />
      ))}
    </span>
  )
}

const copyText = (text: string) => navigator.clipboard?.writeText(text)

/** What every row builder reads from the palette. */
export type RowContext = {
  navigate: ReturnType<typeof useNavigate>
  /** The snapshot's time, which dates activities relative to. */
  referenceIso: string
  /** The trimmed query. */
  query: string
  externalLogos: boolean
  names: FinanceSettings['accountNames']
  accountName: (id: string, name: string) => string
}

/** One activity as a row: shows the entry, searches all of its title, or copies it. */
export function activityRow(activity: ActivityItem, context: RowContext): MenuAction {
  const date = transactionDateKey(activity.date)
  return {
    id: `activity:${activity.id}`,
    label: activity.title,
    detail: [activity.description, activity.account, activity.category, date]
      .filter(Boolean)
      .join(' · '),
    search: `activity ${context.query}`,
    group: 'Activity',
    icon: Activity,
    mark: <ActivityMark activity={activity} />,
    fixedScore: 45,
    ephemeral: true,
    secondary: {
      label: `Search all “${activity.title}”`,
      icon: Search,
      run: () => context.navigate({ to: '/activities', search: { q: activity.title } }),
    },
    extras: [
      {
        label: 'Copy description',
        icon: Copy,
        notice: 'Copied',
        run: () => copyText(activity.title),
      },
    ],
    run: () =>
      context.navigate({
        to: '/activities',
        search: {
          q: `description:${activity.title}`,
          from: validDate(date),
          to: validDate(date),
        },
      }),
  }
}

const objectIcons: Record<ObjectKind, IconComponent> = {
  account: Landmark,
  transaction: Activity,
  holding: TrendingUp,
  trade: ArrowLeftRight,
  document: FileText,
  statement: ReceiptText,
  person: Users,
  organization: Building,
  place: MapPinned,
  trip: Plane,
  bill: Receipt,
  benefit: Gift,
  story: ScrollText,
  chapter: BriefcaseBusiness,
  program: ShieldCheck,
}

/**
 * The object id a palette row names, for its raw view: an object hit, an account, or an
 * Activity entry's transaction or trade.
 */
export function rowObjectId(rowId: string): string | undefined {
  if (rowId.startsWith('object:')) return rowId.slice('object:'.length)
  if (rowId.startsWith('account:')) return rowId
  if (rowId.startsWith('activity:')) return activityObjectId(rowId.slice('activity:'.length))
  return undefined
}

/**
 * One of Rust's search results as a row, keeping Rust's order: word matches, then meaning
 * matches below them. Enter opens its page; ⌘↵ or ⌘I shows its raw view.
 */
export function objectRow(
  hit: SearchHit,
  index: number,
  context: RowContext & Pick<PaletteActionsInput, 'showObject' | 'runVerb'>,
): MenuAction {
  const kind = objectKinds[hit.kind]
  const open = (object: Parameters<typeof openObject>[1]) =>
    openObject(context.navigate, object, context.showObject)
  return {
    id: `object:${hit.id}`,
    label: hit.title,
    detail: [
      kind.label,
      hit.subtitle,
      hit.date ? transactionDateKey(hit.date) : undefined,
      hit.amount == null ? undefined : formatCurrency(hit.amount),
      hit.match === 'meaning' ? 'Similar' : undefined,
    ]
      .filter(Boolean)
      .join(' · '),
    search: '',
    group: 'Objects',
    icon: objectIcons[hit.kind],
    fixedScore: (hit.match === 'words' ? 50 : 40) - index * 0.01,
    ephemeral: true,
    verb: 'Open',
    secondary: {
      label: 'Show details',
      icon: Info,
      run: () => context.showObject(hit.id),
    },
    extras: [
      ...verbRows(hit.kind, hit.id, context.runVerb),
      { label: 'Copy id', icon: Copy, notice: 'Copied', run: () => copyText(hit.id) },
    ],
    // A story opens its link, which only its full record holds.
    run: () =>
      hit.kind === 'story'
        ? getObject(hit.id).then((detail) => (detail ? open(detail.object) : undefined))
        : open(hit),
  }
}

export type PaletteActionsInput = RowContext & {
  finance: FinanceSnapshot | undefined
  registeredActions: readonly AppAction[]
  pages: ReturnType<typeof useOrderedNavigation>
  chips: readonly FilterChip[]
  /** Whether the query or chips ask Activity; and the entries they match. */
  searchesActivity: boolean
  textSearch: boolean
  activityMatches: readonly ActivityItem[]
  /** Search read as many entries as it reads at once; there may be more. */
  activityCapped: boolean
  activityAccountIds: ReadonlySet<string>
  isRefreshing: boolean
  refresh: () => unknown
  onShowShortcuts: () => void
  /** What Rust's search found for the query, best first. */
  objectHits: readonly SearchHit[]
  showObject: (id: string) => void
  /** Opens a catalog verb's page or object, then runs it. */
  runVerb: (verb: Verb, objectId?: string) => unknown
  /** Asks which object a verb acts on. */
  chooseVerb: (verb: Verb) => void
}

/** An object's catalog verbs, for its row's Actions menu. */
const verbRows = (
  kind: ObjectKind,
  objectId: string,
  runVerb: PaletteActionsInput['runVerb'],
): RowAction[] =>
  objectVerbs(kind).map((verb) => ({
    label: verb.title,
    icon: List,
    run: () => runVerb(verb, objectId),
  }))

/**
 * Every row the palette can offer: registered actions first, then an Activity search for the
 * query, pages, charts, accounts, holdings, and a few actions pages may not have registered.
 */
export function paletteActions(input: PaletteActionsInput): MenuAction[] {
  const { finance, registeredActions, pages, chips, navigate, query } = input
  const hasRegistered = (id: string) => registeredActions.some((action) => action.id === id)
  const pageLabel = (href: string) => {
    const path = href.split('?')[0]
    return (
      [
        ...pages,
        ...secondaryNavigation,
        ...analyticsNavigation.map(({ label }) => ({ label, to: '/analytics' })),
      ].find(({ to }) => to === path)?.label ?? path
    )
  }
  return [
    ...registeredActions
      .filter(({ id }) => id !== 'palette.open')
      .map((action) => ({
        id: action.id,
        label: action.title,
        search: action.search ?? action.title,
        group: action.group ?? 'Actions',
        icon: action.icon ?? List,
        detail: action.disabled ? action.disabledReason : action.detail,
        shortcut: action.bindings?.[0]?.label,
        disabled: action.disabled,
        priority: actionScopePriority[action.scope],
        ...(action.id.startsWith('navigation.open.')
          ? { verb: 'Open page', aliases: [action.title.replace(/^Open /, '')] }
          : {}),
        registeredAction: action,
        run: action.run,
      })),
    ...(input.searchesActivity
      ? [
          {
            id: `activity:search:${query}:${chips.map(({ label }) => label).join(',')}`,
            label: input.textSearch ? `Search Activity for “${query}”` : 'Show filtered Activity',
            detail: !finance
              ? 'Local activity unavailable'
              : `${input.activityMatches.length}${input.activityCapped ? '+' : ''} matching ${pluralize(input.activityMatches.length, 'entry', 'entries')}`,
            search: `activity search ${query}`,
            group: 'Activity',
            icon: Search,
            disabled: !finance,
            // Below a word-start hit, so "acc" finds Accounts before searching Activity for it.
            fixedScore: chips.length && !input.textSearch ? 100 : 58,
            verb: 'Search',
            ephemeral: true,
            run: () =>
              navigate({
                to: '/activities',
                search: { ...chipSearch(chips), ...(input.textSearch ? { q: query } : {}) },
              }),
          },
          ...input.activityMatches.slice(0, 8).map((activity) => activityRow(activity, input)),
        ]
      : []),
    ...pages
      .filter(({ to }) => !hasRegistered(`navigation.open.${to}`))
      .map(({ label, to, icon, ...destination }) => ({
        id: `navigate:${to}`,
        label,
        search: `navigate ${label}`,
        group: 'Navigate',
        icon,
        shortcut: 'shortcut' in destination ? destination.shortcut : undefined,
        run: () => navigate({ to }),
      })),
    ...secondaryNavigation.map(({ to, label, icon }) => ({
      id: `navigate:${to}`,
      label,
      search: `navigate ${label}`,
      group: 'Navigate',
      icon,
      run: () => navigate({ to }),
    })),
    ...analyticsNavigation.map(({ label, icon, search }) => ({
      id: `analytics:${search.chart}`,
      label,
      search: `analytics chart ${label}`,
      group: 'Analytics',
      icon,
      run: () => navigate({ to: '/analytics', search }),
    })),
    ...(finance?.accounts ?? [])
      .filter(({ id }) => id !== 'all')
      .map((account) => {
        const label = input.accountName(account.id, account.name)
        return {
          id: `account:${account.id}`,
          label,
          search: `${label} ${account.name} ${account.institution} ${account.type} account`,
          group: 'Accounts',
          icon: account.type === 'credit' ? CreditCard : Landmark,
          mark: (
            <AccountMark
              type={account.type}
              institution={account.institution}
              domain={account.domain}
            />
          ),
          detail: account.institution || undefined,
          secondary: input.activityAccountIds.has(account.id)
            ? {
                label: 'Show account Activity',
                icon: Activity,
                run: () => navigate({ to: '/activities', search: { account: account.id } }),
              }
            : undefined,
          extras: [
            ...verbRows('account', `account:${account.id}`, input.runVerb),
            {
              label: 'Copy account name',
              icon: Copy,
              notice: 'Copied',
              run: () => copyText(label),
            },
          ],
          run: () => navigate({ to: '/accounts', search: { account: account.id } }),
        }
      }),
    ...heldSecurities(finance?.holdings ?? []).map(({ ticker, name }) => ({
      id: `holding:${ticker}`,
      label: `${ticker} · ${name}`,
      search: `${ticker} ${name} holding security`,
      group: 'Holdings',
      icon: TrendingUp,
      mark: tickerMark(ticker, input.externalLogos),
      aliases: [ticker],
      secondary: {
        label: `Search Activity for ${ticker}`,
        icon: Search,
        run: () => navigate({ to: '/activities', search: { q: ticker } }),
      },
      extras: [
        ...verbRows('holding', holdingObjectId(finance?.holdings ?? [], ticker), input.runVerb),
        { label: 'Copy ticker', icon: Copy, notice: 'Copied', run: () => copyText(ticker) },
      ],
      run: () => navigate({ to: '/holdings/$ticker', params: { ticker } }),
    })),
    ...(!hasRegistered('finance.refresh')
      ? [
          {
            id: 'refresh',
            label: input.isRefreshing ? 'Refreshing snapshot…' : 'Refresh snapshot',
            search: 'refresh data snapshot',
            group: 'Actions',
            icon: RefreshCw,
            shortcut: '⌘R',
            disabled: input.isRefreshing || Boolean(finance?.recovery),
            run: input.refresh,
          },
        ]
      : []),
    ...(!hasRegistered('shortcuts.open')
      ? [
          {
            id: 'shortcuts',
            label: 'Keyboard shortcuts',
            search: 'keyboard shortcuts help',
            group: 'Actions',
            icon: HelpCircle,
            shortcut: '⌘/',
            run: input.onShowShortcuts,
          },
        ]
      : []),
    {
      id: 'sources',
      label: 'Review data sources',
      search: 'data sources integrations providers connections',
      group: 'Actions',
      icon: Database,
      run: () => navigate({ to: '.', search: true, hash: settingsHash('connections') }),
    },
    // Verbs whose page isn't open, which open it and then run.
    ...verbs
      .filter((verb) => verb.page && !hasRegistered(verb.id))
      .map((verb) => ({
        id: `verb:${verb.id}`,
        label: verb.title,
        search: `${verb.title} ${verb.keywords}`,
        group: 'Actions',
        icon: List,
        detail: `On ${pageLabel(verb.page ?? '')}`,
        run: () => input.runVerb(verb),
      })),
    // Verbs on an object, which ask which one first; with none to act on, they say so.
    ...verbs.flatMap((verb) => {
      if (!verb.object) return []
      const kind = objectKinds[verb.object]
      const count = objectCount(finance, verb.object)
      return [
        {
          id: `verb:${verb.id}`,
          label: `${verb.title.replace(/…$/, '')}…`,
          search: `${verb.title} ${verb.keywords} ${kind.label}`,
          group: 'Actions',
          icon: objectIcons[verb.object],
          detail: count ? `Choose ${article(kind.label)}` : `No ${kind.plural.toLowerCase()} yet`,
          disabled: !count,
          keepsOpen: true,
          run: () => input.chooseVerb(verb),
        },
      ]
    }),
    ...input.objectHits.map((hit, index) => objectRow(hit, index, input)),
  ]
}

const article = (noun: string) => `${/^[aeiou]/i.test(noun) ? 'an' : 'a'} ${noun.toLowerCase()}`

/**
 * How many objects of a kind the snapshot holds, for the kinds verbs act on; a kind it can't
 * count is taken to have some, and the question lists what search finds.
 */
function objectCount(finance: FinanceSnapshot | undefined, kind: ObjectKind) {
  if (!finance) return 0
  switch (kind) {
    case 'account':
      return finance.accounts.filter(({ id }) => id !== 'all').length
    case 'holding':
      return finance.holdings.length
    case 'document':
      return finance.documents?.length ?? 0
    case 'trip':
      return finance.insights?.trips.trips.length ?? 0
    case 'person':
      return finance.settings?.people?.length ?? 0
    default:
      return 1
  }
}

/** One object a verb could act on, as the verb's question lists it. */
export function choiceRow(
  hit: SearchHit,
  index: number,
  verb: Verb,
  runVerb: PaletteActionsInput['runVerb'],
): MenuAction {
  return {
    id: `choice:${hit.id}`,
    label: hit.title,
    detail: [hit.subtitle, hit.date ? transactionDateKey(hit.date) : undefined]
      .filter(Boolean)
      .join(' · '),
    search: '',
    group: 'Objects',
    icon: objectIcons[hit.kind],
    fixedScore: 50 - index * 0.01,
    ephemeral: true,
    verb: verb.title.replace(/…$/, ''),
    run: () => runVerb(verb, hit.id),
  }
}

/** The object id of a ticker's position: the first account's, as the holding page shows. */
const holdingObjectId = (
  holdings: readonly { ticker: string; accountId: string }[],
  ticker: string,
) => {
  const held = holdings.find((holding) => holding.ticker === ticker)
  return held ? `holding:${held.accountId}:${ticker}` : `holding::${ticker}`
}

/** The `/` filter picker's rows: first a field, then its values; a choice becomes a chip. */
export function filterRows(
  step: FilterStep | undefined,
  input: {
    finance: FinanceSnapshot | undefined
    activities: readonly ActivityItem[]
    activityAccountIds: ReadonlySet<string>
    accountName: (id: string, name: string) => string
    chooseField: (field: FilterField) => void
    addChip: (chip: FilterChip) => void
  },
): MenuAction[] {
  if (!step) return []
  if (step === 'field')
    return filterFields.map(({ field, label, icon }) => ({
      id: `filter-field:${field}`,
      label: `Filter by ${label}`,
      search: `filter ${label}`,
      group: 'Filters',
      icon,
      keepsOpen: true,
      run: () => input.chooseField(field),
    }))
  if (step === 'account')
    return (input.finance?.accounts ?? [])
      .filter(({ id }) => id !== 'all' && input.activityAccountIds.has(id))
      .map((account) => {
        const label = input.accountName(account.id, account.name)
        return {
          id: `filter:account:${account.id}`,
          label,
          detail: account.institution || undefined,
          search: `${label} ${account.name} ${account.institution}`,
          group: 'Filters',
          icon: Landmark,
          mark: (
            <AccountMark
              type={account.type}
              institution={account.institution}
              domain={account.domain}
            />
          ),
          keepsOpen: true,
          run: () => input.addChip({ field: 'account', value: account.id, label }),
        }
      })
  if (step === 'category')
    return [...new Set(input.activities.map(({ category }) => category || 'Other'))]
      .toSorted((left, right) => left.localeCompare(right))
      .map((category) => ({
        id: `filter:category:${category}`,
        label: category,
        search: category,
        group: 'Filters',
        icon: filterFields[1].icon,
        mark: <CategoryMark category={category} />,
        keepsOpen: true,
        run: () => input.addChip({ field: 'category', value: category, label: category }),
      }))
  if (step === 'method')
    return [...new Set(input.activities.map(activityMethod).filter(Boolean))]
      .toSorted((left, right) => left.localeCompare(right))
      .map((method) => ({
        id: `filter:method:${method}`,
        label: method,
        search: method,
        group: 'Filters',
        icon: CreditCard,
        mark: <ActivityMethodMark method={method} />,
        keepsOpen: true,
        run: () => input.addChip({ field: 'method', value: method, label: method }),
      }))
  return datePresets(localDateKey(Date.now() / 1000)).map(({ label, from, to }) => ({
    id: `filter:date:${label}`,
    label,
    detail: from === to ? from : `${from} – ${to}`,
    search: label,
    group: 'Filters',
    icon: filterFields[3].icon,
    keepsOpen: true,
    run: () => input.addChip({ field: 'date', value: from, to, label }),
  }))
}
