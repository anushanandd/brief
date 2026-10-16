import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useId, useState, type Ref } from 'react'

import { useFinanceEdits } from '../hooks/use-finance-edits'
import {
  contactsAccess,
  requestContactsAccess,
  searchContacts,
  type Contact,
} from '../lib/contacts'
import { errorMessage } from '../lib/errors'
import { filterPeople, newPerson, peopleById, withPerson } from '../lib/people'
import type { Person } from '../lib/schema'
import { X } from './icons'
import { PersonAvatar } from './person-avatar'

const MAX_OPTIONS = 8

type Option =
  | { kind: 'person'; person: Person }
  | { kind: 'contact'; contact: Contact }
  | { kind: 'new'; name: string }
  | { kind: 'allow' }

const optionKey = (option: Option) =>
  option.kind === 'person'
    ? option.person.id
    : option.kind === 'contact'
      ? `contact:${option.contact.identifier}`
      : option.kind

/**
 * Finds a Contacts card by any name on it, to link to someone already saved. Their name in
 * Brief stays as it is; the card only lends its photo.
 */
export function ContactLinker({
  label,
  pending,
  onLink,
}: {
  label: string
  pending: boolean
  onLink: (contact: Contact) => Promise<unknown>
}) {
  const client = useQueryClient()
  const listId = useId()
  const [query, setQuery] = useState('')
  const [debounced, setDebounced] = useState('')
  const [active, setActive] = useState(0)
  const access = useContactsAccess().data ?? 'unavailable'
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim()), 150)
    return () => clearTimeout(timer)
  }, [query])
  const contacts = useQuery({
    queryKey: ['contacts', debounced],
    queryFn: () => searchContacts(debounced),
    enabled: access === 'allowed' && debounced.length > 0,
    staleTime: 60_000,
  })
  if (access === 'unavailable')
    return <span className="transaction-edit-note">Contacts are available in the Brief app.</span>
  if (access === 'denied' || access === 'restricted')
    return (
      <span className="transaction-edit-note">
        Contacts access is off in System Settings › Privacy & Security.
      </span>
    )
  if (access === 'notAsked')
    return (
      <button
        type="button"
        className="button-base button-secondary button-compact"
        disabled={pending}
        onClick={() =>
          void requestContactsAccess().then(() =>
            client.invalidateQueries({ queryKey: ['contacts-access'] }),
          )
        }
      >
        Allow Contacts
      </button>
    )
  const found = query.trim() ? (contacts.data ?? []).slice(0, MAX_OPTIONS) : []
  const current = Math.min(active, Math.max(0, found.length - 1))
  const link = (contact: Contact) =>
    void onLink(contact).then(() => {
      setQuery('')
      setActive(0)
    })
  return (
    <span className="people-picker-field">
      <input
        className="transaction-tag-input"
        role="combobox"
        aria-label={label}
        aria-expanded={found.length > 0}
        aria-controls={listId}
        aria-activedescendant={found.length ? `${listId}-${current}` : undefined}
        placeholder="Find in Contacts"
        maxLength={80}
        value={query}
        disabled={pending}
        onChange={(event) => {
          setQuery(event.target.value)
          setActive(0)
        }}
        onKeyDown={(event) => {
          if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
            event.preventDefault()
            if (found.length)
              setActive(
                (current + (event.key === 'ArrowDown' ? 1 : -1) + found.length) % found.length,
              )
          } else if (event.key === 'Enter') {
            event.preventDefault()
            const contact = found[current]
            if (contact) link(contact)
          } else if (event.key === 'Escape' && query) {
            event.stopPropagation()
            setQuery('')
          }
        }}
      />
      {found.length ? (
        <ul className="people-picker-options" id={listId} role="listbox" aria-label={label}>
          {found.map((contact, index) => (
            <li
              key={contact.identifier}
              id={`${listId}-${index}`}
              role="option"
              aria-selected={index === current}
              onPointerDown={(event) => {
                event.preventDefault()
                link(contact)
              }}
              onPointerEnter={() => setActive(index)}
            >
              <PersonAvatar person={{ name: contact.name }} photos={{}} />
              {contact.name}
            </li>
          ))}
        </ul>
      ) : query.trim() && contacts.isSuccess && debounced === query.trim() ? (
        <span className="transaction-edit-note">No card matches.</span>
      ) : null}
    </span>
  )
}

/** Whether Brief may read Contacts, shared by every picker. */
export function useContactsAccess() {
  return useQuery({ queryKey: ['contacts-access'], queryFn: contactsAccess, staleTime: 60_000 })
}

/**
 * Chooses people: the chosen ones as chips, and a field that finds someone saved, someone in
 * Contacts, or adds the name typed. Someone new is saved before they're chosen. ↑ and ↓ move
 * through the matches, Return picks one, and Escape clears the field.
 */
export function PeoplePicker({
  label,
  chosen,
  people,
  photos,
  pending,
  inputRef,
  onChange,
}: {
  label: string
  chosen: readonly string[]
  people: readonly Person[]
  photos: Record<string, string>
  pending: boolean
  inputRef?: Ref<HTMLInputElement>
  onChange: (people: string[]) => Promise<unknown>
}) {
  const { updateSettings } = useFinanceEdits()
  const client = useQueryClient()
  const listId = useId()
  const [query, setQuery] = useState('')
  const [debounced, setDebounced] = useState('')
  const [active, setActive] = useState(0)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string>()
  const access = useContactsAccess().data ?? 'unavailable'
  useEffect(() => {
    const timer = setTimeout(() => setDebounced(query.trim()), 150)
    return () => clearTimeout(timer)
  }, [query])
  const contacts = useQuery({
    queryKey: ['contacts', debounced],
    queryFn: () => searchContacts(debounced),
    enabled: access === 'allowed' && debounced.length > 0,
    staleTime: 60_000,
  })
  const byId = peopleById({ people: [...people] })
  const shown = chosen.flatMap((id) => byId.get(id) ?? [])
  const typed = query.trim()
  const linked = new Set(people.flatMap(({ contactId }) => (contactId ? [contactId] : [])))
  const options: Option[] = typed
    ? [
        ...filterPeople(people, typed)
          .filter(({ id }) => !chosen.includes(id))
          .map((person) => ({ kind: 'person' as const, person })),
        ...(contacts.data ?? [])
          .filter(({ identifier }) => !linked.has(identifier))
          .map((contact) => ({ kind: 'contact' as const, contact })),
        ...(people.some(({ name }) => name.toLowerCase() === typed.toLowerCase())
          ? []
          : [{ kind: 'new' as const, name: typed }]),
        ...(access === 'notAsked' ? [{ kind: 'allow' as const }] : []),
      ].slice(0, MAX_OPTIONS)
    : []
  const current = Math.min(active, Math.max(0, options.length - 1))

  const run = async (action: () => Promise<unknown>) => {
    if (busy || pending) return
    setBusy(true)
    setError(undefined)
    try {
      await action()
    } catch (failure) {
      setError(errorMessage(failure))
    } finally {
      setBusy(false)
    }
  }
  const choose = (id: string) => onChange([...chosen, id])
  const pick = (option: Option) =>
    run(async () => {
      if (option.kind === 'allow') {
        await requestContactsAccess()
        await client.invalidateQueries({ queryKey: ['contacts-access'] })
        return
      }
      if (option.kind === 'person') {
        await choose(option.person.id)
      } else {
        const person =
          option.kind === 'contact'
            ? newPerson(option.contact.name, option.contact.identifier)
            : newPerson(option.name)
        await updateSettings((settings) => withPerson(settings, person))
        await choose(person.id)
      }
      setQuery('')
      setActive(0)
    })

  return (
    <div className="people-picker" aria-busy={busy || pending || undefined}>
      {shown.map((person) => (
        <span className="transaction-tag person-chip" key={person.id}>
          <PersonAvatar person={person} photos={photos} />
          {person.name}
          <button
            type="button"
            aria-label={`Remove ${person.name}`}
            disabled={busy || pending}
            onClick={() => void run(() => onChange(chosen.filter((id) => id !== person.id)))}
          >
            <X size={12} aria-hidden="true" />
          </button>
        </span>
      ))}
      <span className="people-picker-field">
        <input
          ref={inputRef}
          className="transaction-tag-input"
          role="combobox"
          aria-label={label}
          aria-keyshortcuts="P"
          aria-expanded={options.length > 0}
          aria-controls={listId}
          aria-activedescendant={options.length ? `${listId}-${current}` : undefined}
          placeholder="Add person"
          maxLength={80}
          value={query}
          disabled={busy || pending}
          onChange={(event) => {
            setQuery(event.target.value)
            setActive(0)
          }}
          onKeyDown={(event) => {
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault()
              if (options.length)
                setActive(
                  (current + (event.key === 'ArrowDown' ? 1 : -1) + options.length) %
                    options.length,
                )
            } else if (event.key === 'Enter') {
              event.preventDefault()
              const option = options[current]
              if (option) void pick(option)
            } else if (event.key === 'Escape' && query) {
              event.stopPropagation()
              setQuery('')
            }
          }}
        />
        {options.length ? (
          <ul className="people-picker-options" id={listId} role="listbox" aria-label={label}>
            {options.map((option, index) => (
              <li
                key={optionKey(option)}
                id={`${listId}-${index}`}
                role="option"
                aria-selected={index === current}
                onPointerDown={(event) => {
                  // Keep the field focused so the pick lands while it's still open.
                  event.preventDefault()
                  void pick(option)
                }}
                onPointerEnter={() => setActive(index)}
              >
                {option.kind === 'person' ? (
                  <>
                    <PersonAvatar person={option.person} photos={photos} />
                    {option.person.name}
                  </>
                ) : option.kind === 'contact' ? (
                  <>
                    <PersonAvatar person={{ name: option.contact.name }} photos={photos} />
                    {option.contact.name}
                    <small>Contacts</small>
                  </>
                ) : option.kind === 'new' ? (
                  `Add “${option.name}”`
                ) : (
                  'Find people in Contacts…'
                )}
              </li>
            ))}
          </ul>
        ) : null}
      </span>
      {access === 'denied' && typed ? (
        <span className="transaction-edit-note">
          Contacts access is off in System Settings › Privacy & Security.
        </span>
      ) : null}
      {error ? (
        <p className="transaction-edit-error" role="alert">
          {error}
        </p>
      ) : null}
    </div>
  )
}
