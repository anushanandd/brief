import { useQuery } from '@tanstack/react-query'

import { contactPhotos } from '../lib/contacts'
import type { Person } from '../lib/schema'

/**
 * Photos for people added from Contacts, read from Contacts each launch and kept in memory
 * only. Without access, or for anyone added by name, there are none and initials show instead.
 */
export function useContactPhotos(people: readonly Person[]) {
  const ids = [
    ...new Set(people.flatMap(({ contactId }) => (contactId ? [contactId] : []))),
  ].toSorted()
  const photos = useQuery({
    queryKey: ['contact-photos', ids],
    queryFn: () => contactPhotos(ids),
    enabled: ids.length > 0,
    staleTime: Infinity,
    retry: false,
  })
  return photos.data ?? {}
}
