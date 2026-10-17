import { invoke } from '@tauri-apps/api/core'

import { isTauri } from './api'
import type { Contact } from './bindings/Contact'
import type { ContactFit } from './bindings/ContactFit'
import type { ContactsAccess } from './bindings/ContactsAccess'

/**
 * Read-only Mac Contacts, for adding people and showing their photos. Only the app can read
 * Contacts; the browser preview reports it unavailable.
 */
export type { Contact, ContactFit, ContactsAccess }

export async function contactsAccess(): Promise<ContactsAccess> {
  if (!isTauri()) return 'unavailable'
  return invoke<ContactsAccess>('contacts_access')
}

/** Shows macOS's prompt the first time; afterward macOS answers from the saved choice. */
export async function requestContactsAccess(): Promise<ContactsAccess> {
  if (!isTauri()) return 'unavailable'
  return invoke<ContactsAccess>('request_contacts_access')
}

export async function searchContacts(query: string): Promise<Contact[]> {
  if (!isTauri() || !query.trim()) return []
  return invoke<Contact[]>('search_contacts', { query })
}

/** For each name a payment carries, the Contacts cards it could be, likeliest first. */
export async function matchContacts(names: string[]): Promise<Record<string, ContactFit[]>> {
  if (!isTauri() || !names.length) return {}
  return invoke<Record<string, ContactFit[]>>('match_contacts', { names })
}

/** Photos as data URLs, by card identifier; a card without one is left out. */
export async function contactPhotos(identifiers: string[]): Promise<Record<string, string>> {
  if (!isTauri() || !identifiers.length) return {}
  const photos = await invoke<Record<string, string>>('contact_photos', { identifiers })
  return Object.fromEntries(
    Object.entries(photos).map(([id, data]) => [id, `data:image/jpeg;base64,${data}`]),
  )
}
