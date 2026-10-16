import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Link, useNavigate, useSearch } from '@tanstack/react-router'
import { useCallback, useMemo, useRef, useState } from 'react'

import { ActivityList } from '../components/activity-list'
import { FilterSelect } from '../components/filter-select'
import { Plane, Plus, X } from '../components/icons'
import { Keys } from '../components/keys'
import { ContactLinker, PeoplePicker, useContactsAccess } from '../components/people-picker'
import { PersonAvatar } from '../components/person-avatar'
import { TransactionDialog } from '../components/transaction-dialog'
import { Button, Card, EmptyState, Metric, ScrollCueCard, SectionHeading } from '../components/ui'
import { useContactPhotos } from '../hooks/use-contact-photos'
import { useFinanceEdits } from '../hooks/use-finance-edits'
import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { ActivityItem } from '../lib/activity'
import { matchContacts, requestContactsAccess } from '../lib/contacts'
import { errorMessage } from '../lib/errors'
import { countOf, formatCurrency } from '../lib/format'
import {
  identities,
  monthlyMoments,
  personColor,
  personFromSearch,
  timeAgo,
  withIdentity,
  withPerson,
  withoutPerson,
  type Identity,
} from '../lib/people'
import type { FinanceSnapshot, Insights, Person } from '../lib/schema'
import { formatListDate } from '../lib/spending'
import { tripDates, tripFlags, tripTitle } from '../lib/trips'

type Activities = Map<string, ActivityItem>
type PersonSummary = Insights['people']['people'][number]
type Suggestion = Insights['people']['suggestions'][number]

const personSearch = (person: Pick<Person, 'id'>) => ({
  view: 'people' as const,
  person: person.id,
})

/**
 * Insights' People view: who transactions and trips were with. Payments to and from someone are
 * tagged by the names their bank prints; anything else is tagged by hand. Tagging never changes
 * an amount or a category, and Rust found every figure here.
 */
export function PeopleView({
  data,
  activities,
}: {
  data: FinanceSnapshot
  activities: Activities
}) {
  const search = useSearch({ from: '/insights' })
  const [openTransaction, setOpenTransaction] = useState<string>()
  const person = personFromSearch(data, search.person)
  const people = data.settings?.people ?? []
  const photos = useContactPhotos(people)
  return (
    <>
      {person ? (
        <PersonView
          key={person.id}
          data={data}
          person={person}
          photos={photos}
          activities={activities}
          onOpenTransaction={setOpenTransaction}
        />
      ) : (
        <PeopleList data={data} photos={photos} />
      )}
      <TransactionDialog
        activity={openTransaction ? activities.get(openTransaction) : undefined}
        data={data}
        onClose={() => setOpenTransaction(undefined)}
      />
    </>
  )
}

/**
 * The roster: names in payments to settle, a line each, then everyone as one table, most
 * recent first.
 */
function PeopleList({ data, photos }: { data: FinanceSnapshot; photos: Record<string, string> }) {
  const navigate = useNavigate({ from: '/insights' })
  const people = data.settings?.people ?? []
  const summaries = data.insights?.people.people ?? []
  const suggestions = data.insights?.people.suggestions ?? []
  const byId = new Map(people.map((person) => [person.id, person]))
  const field = useRef<HTMLInputElement>(null)
  const access = useContactsAccess().data

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'people.add',
          title: 'Find or add a person',
          search: 'people person new add contact friend',
          group: 'Here',
          scope: 'page',
          bindings: [{ sequence: ['n'], label: 'N' }],
          run: () => field.current?.focus(),
        },
        ...people.map((person) => ({
          id: `people.open.${person.id}`,
          title: `Open ${person.name}`,
          search: `person people ${person.name}`,
          group: 'Here',
          scope: 'page' as const,
          run: () => void navigate({ search: personSearch(person) }),
        })),
      ],
      [navigate, people],
    ),
  )

  const today = data.insights?.today ?? data.updatedAt.slice(0, 10)
  const most = Math.max(0, ...summaries.flatMap(({ sent, received }) => [Math.abs(sent), received]))

  return (
    <div className="trips-main">
      {suggestions.length ? (
        <WhoIsThis data={data} suggestions={suggestions} people={people} photos={photos} />
      ) : null}
      <Card className="people-card">
        <SectionHeading
          title="People"
          action={
            <PeoplePicker
              label="Find or add a person"
              chosen={[]}
              people={people}
              photos={photos}
              pending={false}
              inputRef={field}
              onChange={async ([id]) => {
                if (id) await navigate({ search: { view: 'people', person: id } })
              }}
            />
          }
        />
        {access === 'unavailable' ? (
          <p className="transaction-edit-note">Contacts are available in the Brief app.</p>
        ) : access === 'denied' || access === 'restricted' ? (
          <p className="transaction-edit-note">
            Contacts access is off in System Settings › Privacy & Security.
          </p>
        ) : null}
        {summaries.length ? (
          <Roster
            summaries={summaries}
            people={byId}
            photos={photos}
            today={today}
            most={most}
            onOpen={(person) => void navigate({ search: personSearch(person) })}
          />
        ) : (
          <EmptyState>
            {data.insights
              ? 'Add someone you pay or travel with. Payments that name them are tagged by name.'
              : 'People are unavailable for this snapshot.'}
          </EmptyState>
        )}
      </Card>
    </div>
  )
}

/**
 * Everyone as one table, most recent first. Sent and received share one centre line down the
 * whole table, drawn to one scale, so who you exchange the most with reads at a glance. Return
 * on a row opens that person.
 */
function Roster({
  summaries,
  people,
  photos,
  today,
  most,
  onOpen,
}: {
  summaries: PersonSummary[]
  people: Map<string, Person>
  photos: Record<string, string>
  today: string
  most: number
  onOpen: (person: Person) => void
}) {
  const share = (value: number) => `${most ? Math.max(value ? 3 : 0, (value / most) * 100) : 0}%`
  return (
    <table className="people-roster" aria-label="People" data-keyboard-region>
      <thead>
        <tr>
          <th scope="col">Person</th>
          <th scope="col">Last together</th>
          <th scope="col" className="is-number">
            Tagged
          </th>
          <th scope="col" className="is-number">
            Trips
          </th>
          <th scope="col" className="is-sent">
            Sent
          </th>
          <th scope="col" className="is-received">
            Received
          </th>
        </tr>
      </thead>
      <tbody>
        {summaries.flatMap((summary) => {
          const person = people.get(summary.id)
          if (!person) return []
          const sent = Math.abs(summary.sent)
          return [
            <tr
              key={person.id}
              data-keyboard-row
              tabIndex={-1}
              style={{ '--person-color': personColor(person.id) }}
              onClick={(event) => {
                // The name is a link of its own; anywhere else on the row opens them too.
                if (!(event.target instanceof Element) || !event.target.closest('a')) onOpen(person)
              }}
            >
              <td>
                <Link
                  to="/insights"
                  search={personSearch(person)}
                  className="people-roster-person"
                  data-keyboard-open
                >
                  <PersonAvatar person={person} photos={photos} size="medium" />
                  <strong>{person.name}</strong>
                </Link>
              </td>
              <td className="is-muted">
                {summary.last ? timeAgo(summary.last, today) : 'Nothing tagged yet'}
              </td>
              <td className="is-number">{summary.transactionCount}</td>
              <td className="is-number">{summary.tripIds.length || null}</td>
              <td className="is-sent">
                <span className="people-roster-bar">
                  {sent ? <small className="sensitive-value">{formatCurrency(sent)}</small> : null}
                  <span style={{ width: share(sent) }} aria-hidden="true" />
                </span>
              </td>
              <td className="is-received">
                <span className="people-roster-bar">
                  <span style={{ width: share(summary.received) }} aria-hidden="true" />
                  {summary.received ? (
                    <small className="sensitive-value">{formatCurrency(summary.received)}</small>
                  ) : null}
                </span>
              </td>
            </tr>,
          ]
        })}
      </tbody>
    </table>
  )
}

const identityName = (identity: Identity) =>
  identity.kind === 'person' ? identity.person.name : identity.contact.name

/**
 * Names Zelle payments carry that match nobody, each with who it could be: people already
 * saved and Contacts cards, by the same rules Rust matches with. Picking one makes that name
 * theirs, so every payment carrying it is tagged; Y takes the first row's best guess.
 */
function WhoIsThis({
  data,
  suggestions,
  people,
  photos,
}: {
  data: FinanceSnapshot
  suggestions: Suggestion[]
  people: Person[]
  photos: Record<string, string>
}) {
  const { updateSettings } = useFinanceEdits()
  const client = useQueryClient()
  const [pending, setPending] = useState<string>()
  const [error, setError] = useState<string>()
  const access = useContactsAccess().data
  const names = suggestions.map(({ matchName }) => matchName)
  const contacts = useQuery({
    queryKey: ['contact-matches', names],
    queryFn: () => matchContacts(names),
    enabled: access === 'allowed',
    staleTime: 60_000,
  })
  const cardPhotos = useContactPhotos(
    Object.values(contacts.data ?? {})
      .flat()
      .filter(({ hasPhoto }) => hasPhoto)
      .map(({ identifier, name }) => ({ id: identifier, name, contactId: identifier })),
  )
  const faces = { ...cardPhotos, ...photos }
  const transactions = new Map(
    data.transactions.map((transaction) => [transaction.id, transaction]),
  )
  const choices = people
    .map(({ id, name }) => ({ value: id, label: name }))
    .toSorted((left, right) => left.label.localeCompare(right.label))
  const rows = suggestions.map((suggestion) => ({
    suggestion,
    options: identities(suggestion, people, contacts.data?.[suggestion.matchName]),
  }))
  const save = useCallback(
    async (suggestion: Suggestion, identity: Parameters<typeof withIdentity>[2]) => {
      if (pending) return
      setPending(suggestion.matchName)
      setError(undefined)
      try {
        await updateSettings((settings) => withIdentity(settings, suggestion.matchName, identity))
        await client.invalidateQueries({ queryKey: ['contact-matches'] })
      } catch (failure) {
        setError(errorMessage(failure))
      } finally {
        setPending(undefined)
      }
    },
    [client, pending, updateSettings],
  )
  const first = rows.find(({ options }) => options.length)

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'people.contacts.allow',
          title: 'Look for names in payments in Contacts',
          search: 'people contacts allow access match names',
          group: 'Here',
          scope: 'page',
          disabled: access !== 'notAsked',
          disabledReason: 'Contacts access was already decided',
          run: () =>
            void requestContactsAccess().then(() =>
              client.invalidateQueries({ queryKey: ['contacts-access'] }),
            ),
        },
        {
          id: 'people.who.accept',
          title: first
            ? `${first.suggestion.name} is ${identityName(first.options[0])}`
            : 'Accept the best match',
          search: 'people match name payment zelle who is this yes accept',
          group: 'Here',
          scope: 'page',
          bindings: [{ sequence: ['y'], label: 'Y' }],
          disabled: !first,
          disabledReason: 'No name in payments has a match',
          run: () => {
            if (first) void save(first.suggestion, first.options[0])
          },
        },
        ...rows.flatMap(({ suggestion, options }) =>
          options.map((identity) => ({
            id: `people.who.${suggestion.matchName}.${identityName(identity)}`,
            title: `${suggestion.name} is ${identityName(identity)}`,
            search: `people match name payment zelle ${suggestion.name} ${identityName(identity)}`,
            group: 'Here',
            scope: 'page' as const,
            run: () => void save(suggestion, identity),
          })),
        ),
      ],
      [access, client, first, rows, save],
    ),
  )

  return (
    <ScrollCueCard className="people-who-card" scrollSelector=".people-who">
      <SectionHeading title="Who’s this?" />
      <ul className="people-who" aria-label="Names in payments" data-keyboard-region>
        {rows.map(({ suggestion, options }) => {
          const latest = transactions.get(suggestion.transactionIds[0])
          return (
            <li
              key={suggestion.matchName}
              data-keyboard-row
              tabIndex={-1}
              aria-busy={pending === suggestion.matchName || undefined}
            >
              <span className="people-who-name">
                <span className="person-avatar is-unknown" aria-hidden="true">
                  ?
                </span>
                <strong>{suggestion.name}</strong>
                <small>
                  {countOf(suggestion.transactionIds.length, 'Zelle payment')}
                  {latest ? (
                    <>
                      {` · last ${latest.amount < 0 ? 'sent' : 'received'} `}
                      <span className="sensitive-value">
                        {formatCurrency(Math.abs(latest.amount))}
                      </span>
                      {` ${formatListDate(latest.postedOn ?? latest.date, data.updatedAt)}`}
                    </>
                  ) : null}
                </small>
                <span className="people-who-arrow" aria-hidden="true">
                  →
                </span>
              </span>
              <span
                className="people-who-options"
                role="group"
                aria-label={`Who ${suggestion.name} is`}
              >
                {options.map((identity, index) => {
                  const shown =
                    identity.kind === 'person'
                      ? identity.person
                      : { name: identity.contact.name, contactId: identity.contact.identifier }
                  const id =
                    identity.kind === 'person' ? identity.person.id : identity.contact.identifier
                  return (
                    <button
                      key={id}
                      type="button"
                      className={`people-who-option${index === 0 && first?.suggestion === suggestion ? ' is-first' : ''}`}
                      style={{ '--person-color': personColor(id) }}
                      disabled={Boolean(pending)}
                      onClick={() => void save(suggestion, identity)}
                    >
                      <PersonAvatar person={shown} photos={faces} />
                      <span>
                        <strong>{shown.name}</strong>
                        <small>
                          {`${identity.kind === 'person' ? 'In People' : 'Contacts'} · ${identity.fit === 'likely' ? 'Likely' : 'Maybe'}`}
                        </small>
                      </span>
                    </button>
                  )
                })}
                <button
                  type="button"
                  className="people-who-option is-new"
                  disabled={Boolean(pending)}
                  onClick={() => void save(suggestion, { kind: 'new', name: suggestion.name })}
                >
                  <Plus size={14} />
                  <span>
                    <strong>{`New person`}</strong>
                    <small>{suggestion.name}</small>
                  </span>
                </button>
                {choices.length ? (
                  <FilterSelect
                    label={`Someone else is ${suggestion.name}`}
                    value=""
                    options={[{ value: '', label: 'Someone else…' }, ...choices]}
                    disabled={Boolean(pending)}
                    onValueChange={(id) => {
                      const person = people.find((candidate) => candidate.id === id)
                      if (person) void save(suggestion, { kind: 'person', person, fit: 'possible' })
                    }}
                  />
                ) : null}
              </span>
            </li>
          )
        })}
      </ul>
      {access === 'notAsked' ? (
        <p className="transaction-edit-note">
          <button
            type="button"
            className="button-base button-ghost button-compact"
            data-command="people.contacts.allow"
            onClick={() =>
              void requestContactsAccess().then(() =>
                client.invalidateQueries({ queryKey: ['contacts-access'] }),
              )
            }
          >
            Look in Contacts too
          </button>
        </p>
      ) : contacts.isFetching ? (
        <p className="transaction-edit-note">Looking in Contacts…</p>
      ) : null}
      {first ? (
        <p className="trips-year-note">
          <Keys label="Y" /> {`${first.suggestion.name} is ${identityName(first.options[0])}`}
        </p>
      ) : null}
      {error ? (
        <p className="transaction-edit-error" role="alert">
          {error}
        </p>
      ) : null}
    </ScrollCueCard>
  )
}

/** One person: what you sent and received, everything tagged with them, and their trips. */
function PersonView({
  data,
  person,
  photos,
  activities,
  onOpenTransaction,
}: {
  data: FinanceSnapshot
  person: Person
  photos: Record<string, string>
  activities: Activities
  onOpenTransaction: (id: string) => void
}) {
  const navigate = useNavigate({ from: '/insights' })
  const [details, setDetails] = useState(false)
  const summaries = data.insights?.people.people ?? []
  const summary = summaries.find(({ id }) => id === person.id)
  const index = summaries.findIndex(({ id }) => id === person.id)
  const earlier = index > 0 ? summaries[index - 1] : undefined
  const later = index >= 0 ? summaries[index + 1] : undefined
  const saved = data.settings?.people
  const names = useMemo(() => new Map((saved ?? []).map(({ id, name }) => [id, name])), [saved])
  const trips = (data.insights?.trips.trips ?? []).filter(({ id }) => summary?.tripIds.includes(id))
  const rows = data.transactions
    .filter((transaction) => transaction.people?.includes(person.id))
    .flatMap(({ id }) => activities.get(`spending:${id}`) ?? [])
    .toSorted((left, right) => right.date.localeCompare(left.date))
  const go = useCallback(
    (next?: PersonSummary) => {
      if (next) void navigate({ search: { view: 'people', person: next.id } })
    },
    [navigate],
  )
  const back = useCallback(() => void navigate({ search: { view: 'people' } }), [navigate])

  useRegisterActions(
    useMemo<AppAction[]>(
      () => [
        {
          id: 'people.earlier',
          title: 'Previous person',
          detail: earlier ? names.get(earlier.id) : undefined,
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['ArrowLeft'], label: '←' }],
          disabled: !earlier,
          disabledReason: 'This is the first person',
          run: () => go(earlier),
        },
        {
          id: 'people.later',
          title: 'Next person',
          detail: later ? names.get(later.id) : undefined,
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['ArrowRight'], label: '→' }],
          disabled: !later,
          disabledReason: 'This is the last person',
          run: () => go(later),
        },
        {
          id: 'people.back',
          title: 'Back to People',
          group: 'View',
          scope: 'page',
          bindings: [{ sequence: ['Escape'], label: 'Esc' }],
          run: back,
        },
        {
          id: 'people.details',
          title: details ? `Hide ${person.name}’s details` : `Show ${person.name}’s details`,
          search: 'person details match names rename remove contacts',
          group: 'Here',
          scope: 'page',
          bindings: [{ sequence: ['d'], label: 'D' }],
          run: () => setDetails((open) => !open),
        },
      ],
      [back, details, earlier, go, later, names, person.name],
    ),
  )

  const today = data.insights?.today ?? data.updatedAt.slice(0, 10)
  const line = [
    summary?.first ? `Together since ${formatListDate(summary.first, data.updatedAt)}` : null,
    summary?.last ? `last ${timeAgo(summary.last, today)}` : 'Nothing tagged yet',
  ]
    .filter(Boolean)
    .join(' · ')
  const moments = monthlyMoments(
    data.transactions
      .filter((transaction) => transaction.people?.includes(person.id))
      .map((transaction) => transaction.postedOn ?? transaction.date),
    today,
  )
  const tripMonths = new Set(trips.map(({ start }) => start.slice(0, 7)))

  return (
    <div className="trips-grid">
      <div className="trips-main">
        <Card className="people-hero-card" style={{ '--person-color': personColor(person.id) }}>
          <div className="trips-year-top">
            <header className="people-hero">
              <PersonAvatar person={person} photos={photos} size="hero" />
              <span>
                <h2 className="people-name">{person.name}</h2>
                <p className="trips-year-line">{line}</p>
              </span>
            </header>
            <Button
              size="compact"
              data-command="people.details"
              aria-expanded={details}
              aria-keyshortcuts="D"
              onClick={() => setDetails((open) => !open)}
            >
              Details
            </Button>
          </div>
          <div className="people-hero-stats">
            <Metric label="Transactions" value={String(summary?.transactionCount ?? 0)} />
            <Metric label="Trips" value={String(trips.length)} />
            <Metric
              label="Sent"
              value={
                <span className="sensitive-value">
                  {formatCurrency(Math.abs(summary?.sent ?? 0))}
                </span>
              }
            />
            <Metric
              label="Received"
              value={
                <span className="sensitive-value">{formatCurrency(summary?.received ?? 0)}</span>
              }
            />
          </div>
          <Moments moments={moments} tripMonths={tripMonths} name={person.name} />
          <p className="trips-year-note">
            Sent and received count payments through Zelle and payment apps.
          </p>
          {details ? <PersonDetails person={person} onRemoved={back} /> : null}
        </Card>
        <Card className="trips-card">
          <SectionHeading title="Activity" />
          {rows.length ? (
            <ActivityList
              activities={rows}
              referenceIso={data.updatedAt}
              onOpen={(activity) => onOpenTransaction(activity.id)}
            />
          ) : (
            <EmptyState>
              {person.matchNames?.length
                ? 'Nothing tagged yet. Payments naming them are tagged as they arrive; press P on any transaction to tag it.'
                : 'Nothing tagged yet. Press P on any transaction to tag it.'}
            </EmptyState>
          )}
        </Card>
      </div>
      <Card className="trips-places-card">
        <SectionHeading title="Trips" />
        {trips.length ? (
          <ul
            className="people-postcards"
            aria-label={`Trips with ${person.name}`}
            data-keyboard-region
          >
            {trips.map((trip, position) => (
              <li
                key={trip.id}
                data-keyboard-row
                tabIndex={-1}
                style={{ '--postcard-tilt': `${position % 2 ? 1.2 : -1.2}deg` }}
              >
                <Link to="/insights" search={{ trip: trip.id }} className="people-postcard">
                  <span className="people-postcard-stamp" aria-hidden="true">
                    {tripFlags(trip) || <Plane size={15} />}
                  </span>
                  <strong>{tripTitle(trip)}</strong>
                  <small>{tripDates(trip, data.updatedAt)}</small>
                </Link>
              </li>
            ))}
          </ul>
        ) : (
          <EmptyState>Add them to a trip from the trip, with P.</EmptyState>
        )}
      </Card>
    </div>
  )
}

/**
 * The last twelve months with someone, a dot a month sized by what's tagged, and a plane on
 * the months a trip together began.
 */
function Moments({
  moments,
  tripMonths,
  name,
}: {
  moments: ReturnType<typeof monthlyMoments>
  tripMonths: ReadonlySet<string>
  name: string
}) {
  const most = Math.max(1, ...moments.map(({ count }) => count))
  const summary = moments
    .filter(({ count }) => count)
    .map(({ label, count }) => `${label}: ${count}`)
    .join(', ')
  return (
    <figure className="people-moments">
      <figcaption>The last twelve months</figcaption>
      <ol
        aria-label={`${name}, the last twelve months${summary ? `. ${summary}` : ': nothing tagged'}`}
      >
        {moments.map((month) => (
          <li
            key={month.key}
            title={`${month.label}: ${countOf(month.count, 'transaction')}`}
            aria-hidden="true"
          >
            <span className="people-moment-plot">
              {tripMonths.has(month.key) ? <Plane size={12} /> : null}
              <span
                className={`people-moment${month.count ? '' : ' is-empty'}`}
                style={{ '--moment-scale': month.count ? 0.35 + (0.65 * month.count) / most : 0.2 }}
              />
            </span>
            <small>{month.label}</small>
          </li>
        ))}
      </ol>
    </figure>
  )
}

/** Renaming, the names banks print for them, their Contacts card, and removing them. */
function PersonDetails({ person, onRemoved }: { person: Person; onRemoved: () => void }) {
  const { updateSettings } = useFinanceEdits()
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<string>()
  const [name, setName] = useState<string>()
  const [matchName, setMatchName] = useState('')
  const [confirming, setConfirming] = useState(false)
  const matchNames = person.matchNames ?? []
  const save = async (change: Parameters<typeof updateSettings>[0]) => {
    if (pending) return false
    setPending(true)
    setError(undefined)
    try {
      await updateSettings(change)
      return true
    } catch (failure) {
      setError(errorMessage(failure))
      return false
    } finally {
      setPending(false)
    }
  }
  const update = (patch: Partial<Person>) =>
    save((settings) => withPerson(settings, { ...person, ...patch }))
  const commitName = () => {
    if (name === undefined) return
    const next = name.trim()
    setName(undefined)
    if (next && next !== person.name) void update({ name: next })
  }
  return (
    <section
      className="transaction-edits people-details"
      aria-label="Details"
      aria-busy={pending || undefined}
    >
      <div className="transaction-edit-row">
        <span>Name</span>
        <div>
          <input
            className="transaction-tag-input people-name-input"
            aria-label="Name"
            maxLength={80}
            value={name ?? person.name}
            disabled={pending}
            onChange={(event) => setName(event.target.value)}
            onBlur={commitName}
            onKeyDown={(event) => {
              if (event.key === 'Enter') commitName()
              if (event.key === 'Escape' && name !== undefined) {
                event.stopPropagation()
                setName(undefined)
              }
            }}
          />
        </div>
      </div>
      <div className="transaction-edit-row">
        <span>Matches</span>
        <div>
          {matchNames.map((item) => (
            <span className="transaction-tag" key={item}>
              {item}
              <button
                type="button"
                aria-label={`Stop matching ${item}`}
                disabled={pending}
                onClick={() =>
                  void update({ matchNames: matchNames.filter((other) => other !== item) })
                }
              >
                <X size={12} aria-hidden="true" />
              </button>
            </span>
          ))}
          <form
            onSubmit={(event) => {
              event.preventDefault()
              const next = matchName.trim()
              if (!next) return
              void update({ matchNames: [...matchNames, next] }).then((saved) => {
                if (saved) setMatchName('')
              })
            }}
          >
            <input
              className="transaction-tag-input"
              aria-label="Add a name banks print"
              placeholder="Add a name"
              maxLength={60}
              value={matchName}
              disabled={pending || matchNames.length >= 8}
              onChange={(event) => setMatchName(event.target.value)}
            />
          </form>
          <span className="transaction-edit-note">
            {matchNames.length
              ? 'Zelle and payment-app transactions naming these are tagged with them.'
              : 'Without a name to match, only transactions you tag are theirs.'}
          </span>
        </div>
      </div>
      <div className="transaction-edit-row">
        <span>Contacts</span>
        <div>
          {person.contactId ? (
            <>
              <span className="transaction-edit-note">Photo from their Contacts card</span>
              <Button
                size="compact"
                variant="ghost"
                disabled={pending}
                onClick={() => void update({ contactId: undefined })}
              >
                Unlink
              </Button>
            </>
          ) : (
            <ContactLinker
              label={`Link ${person.name} to a Contacts card`}
              pending={pending}
              onLink={(contact) => update({ contactId: contact.identifier })}
            />
          )}
        </div>
      </div>
      <div className="transaction-edit-row">
        <span>Remove</span>
        <div>
          {confirming ? (
            <>
              <span className="transaction-edit-note">{`Remove ${person.name}? Their tags stop showing; transactions are unchanged.`}</span>
              <Button
                size="compact"
                variant="ghost"
                autoFocus
                disabled={pending}
                onClick={() => setConfirming(false)}
              >
                Cancel
              </Button>
              <Button
                size="compact"
                variant="destructive"
                disabled={pending}
                onClick={() =>
                  void save((settings) => withoutPerson(settings, person.id)).then((removed) => {
                    if (removed) onRemoved()
                  })
                }
              >
                {pending ? 'Removing…' : 'Remove'}
              </Button>
            </>
          ) : (
            <Button
              size="compact"
              variant="ghost"
              disabled={pending}
              onClick={() => setConfirming(true)}
            >
              Remove person
            </Button>
          )}
        </div>
      </div>
      {error ? (
        <p className="transaction-edit-error" role="alert">
          {error}
        </p>
      ) : null}
    </section>
  )
}
