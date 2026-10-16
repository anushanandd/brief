import { invoke } from '@tauri-apps/api/core'
import { z } from 'zod'

import { isTauri } from './api'

/**
 * Read-only Mac Contacts, for adding people and showing their photos. Only the app can read
 * Contacts; the browser preview reports it unavailable.
 */
export const contactsAccessSchema = z.enum([
  'notAsked',
  'restricted',
  'denied',
  'allowed',
  'unavailable',
])
export type ContactsAccess = z.infer<typeof contactsAccessSchema>

const contactSchema = z.object({
  identifier: z.string(),
  name: z.string(),
  hasPhoto: z.boolean().default(false),
})
export type Contact = z.infer<typeof contactSchema>

export async function contactsAccess(): Promise<ContactsAccess> {
  if (!isTauri()) return 'unavailable'
  return contactsAccessSchema.parse(await invoke('contacts_access'))
}

/** Shows macOS's prompt the first time; afterward macOS answers from the saved choice. */
export async function requestContactsAccess(): Promise<ContactsAccess> {
  if (!isTauri()) return 'unavailable'
  return contactsAccessSchema.parse(await invoke('request_contacts_access'))
}

export async function searchContacts(query: string): Promise<Contact[]> {
  if (!isTauri() || !query.trim()) return []
  return contactSchema.array().parse(await invoke('search_contacts', { query }))
}

const contactFitSchema = contactSchema.extend({ fit: z.enum(['likely', 'possible']) })
export type ContactFit = z.infer<typeof contactFitSchema>

/** For each name a payment carries, the Contacts cards it could be, likeliest first. */
export async function matchContacts(names: string[]): Promise<Record<string, ContactFit[]>> {
  if (!isTauri() || !names.length) return {}
  return z
    .record(z.string(), contactFitSchema.array())
    .parse(await invoke('match_contacts', { names }))
}

/** Photos as data URLs, by card identifier; a card without one is left out. */
export async function contactPhotos(identifiers: string[]): Promise<Record<string, string>> {
  if (!isTauri() || !identifiers.length) return {}
  const photos = z
    .record(z.string(), z.string())
    .parse(await invoke('contact_photos', { identifiers }))
  return Object.fromEntries(
    Object.entries(photos).map(([id, data]) => [id, `data:image/jpeg;base64,${data}`]),
  )
}
