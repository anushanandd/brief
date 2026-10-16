import { initials } from '../lib/people'
import type { Person } from '../lib/schema'

/**
 * A person's Contacts photo, or their initials, ringed in their color where a stylesheet sets
 * `--person-color`. Decorative: their name is always beside it or in its link's label.
 */
export function PersonAvatar({
  person,
  photos,
  size = 'small',
}: {
  person: Pick<Person, 'name' | 'contactId'>
  photos: Record<string, string>
  size?: 'small' | 'medium' | 'hero'
}) {
  const photo = person.contactId ? photos[person.contactId] : undefined
  return (
    <span className={`person-avatar is-${size}`} aria-hidden="true">
      {photo ? <img src={photo} alt="" /> : initials(person.name)}
    </span>
  )
}
