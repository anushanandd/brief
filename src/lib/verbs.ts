import type { ObjectKind } from './objects'

/**
 * The verbs reachable from anywhere, defined once. A page registers each verb's work while
 * it's open (`useRegisterActions`); this catalog says where that page is, so the palette,
 * an object's actions, and `brief://do/<verb>` links can open it and then run the verb, from
 * any page. A verb with an `object` kind runs on one object of that kind; the rest belong to
 * a page. Verbs that act on a list's selection or a view's moment stay with their page.
 */
export type Verb = {
  id: string
  title: string
  keywords: string
  /** The page whose registration does the work, for a verb without an object. */
  page?: string
  /** The kind of object it acts on; opening the object opens its page. */
  object?: ObjectKind
}

export const verbs: readonly Verb[] = [
  {
    id: 'people.add',
    title: 'Find or add a person',
    keywords: 'people contacts new person',
    page: '/insights?view=people',
  },
  {
    id: 'documents.add-files',
    title: 'Add documents…',
    keywords: 'import upload pay statement w-2 1099 pdf',
    page: '/documents',
  },
  {
    id: 'documents.add-folder',
    title: 'Add a folder of documents…',
    keywords: 'import folder documents',
    page: '/documents',
  },
  { id: 'calendar.today', title: 'Go to today', keywords: 'calendar today', page: '/calendar' },
  {
    id: 'activity.review',
    title: 'Show activity to review',
    keywords: 'review flagged duplicates unusual',
    page: '/activities',
  },
  {
    id: 'activity.export',
    title: 'Download filtered activity as CSV',
    keywords: 'export csv spreadsheet transactions',
    page: '/activities',
  },
  {
    id: 'benefits.review',
    title: 'Review benefit matches',
    keywords: 'card credits benefits review matches',
    page: '/analytics?chart=amex-credits',
  },
  {
    id: 'account.export',
    title: 'Download the account’s activity as CSV',
    keywords: 'export csv account',
    object: 'account',
  },
  {
    id: 'account.details.toggle',
    title: 'Show or hide the account’s details',
    keywords: 'account details',
    object: 'account',
  },
  {
    id: 'account.statements.import',
    title: 'Import a statement into the account',
    keywords: 'statement csv xlsx import history',
    object: 'account',
  },
  {
    id: 'documents.view',
    title: 'View the original',
    keywords: 'document pdf',
    object: 'document',
  },
  {
    id: 'documents.open-original',
    title: 'Open the original in Preview',
    keywords: 'document preview pdf',
    object: 'document',
  },
  {
    id: 'documents.reviewed',
    title: 'Mark checked or not checked',
    keywords: 'document reviewed checked',
    object: 'document',
  },
  {
    id: 'documents.reread',
    title: 'Read the document again',
    keywords: 'document reread',
    object: 'document',
  },
  { id: 'documents.delete', title: 'Delete document…', keywords: 'remove', object: 'document' },
  {
    id: 'holding.taxes',
    title: 'Show taxes',
    keywords: 'lots long-term short-term gain',
    object: 'holding',
  },
  {
    id: 'holding.news.refresh',
    title: 'Refresh news',
    keywords: 'news stories',
    object: 'holding',
  },
  {
    id: 'holding.chart.details',
    title: 'Show price chart details',
    keywords: 'chart feed',
    object: 'holding',
  },
  { id: 'trips.people', title: 'Who went on this trip', keywords: 'companions', object: 'trip' },
  { id: 'people.details', title: 'Show or hide details', keywords: 'person', object: 'person' },
]

export const verbById = (id: string) => verbs.find((verb) => verb.id === id)

/** The verbs that act on one kind of object. */
export const objectVerbs = (kind: ObjectKind) => verbs.filter((verb) => verb.object === kind)

/**
 * A `brief://do/<verb>?object=<id>` link, as the app receives it (`/do/<verb>?object=<id>`):
 * the verb and the object it names. Only catalog verbs run from a link.
 */
export function verbLink(view: string): { verb: Verb; object?: string } | undefined {
  const match = /^\/do\/([\w.-]+)(?:\?(.*))?$/.exec(view)
  const verb = match ? verbById(match[1]) : undefined
  if (!verb) return undefined
  const object = new URLSearchParams(match?.[2] ?? '').get('object') ?? undefined
  if (verb.object && !object?.startsWith(`${verb.object}:`)) return undefined
  return { verb, ...(object ? { object } : {}) }
}
