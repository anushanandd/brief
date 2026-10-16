import type { Transaction } from './schema'

export type Place = NonNullable<Transaction['place']>

const countryNames = new Intl.DisplayNames(['en'], { type: 'region' })

/** A country's name from its ISO code, or the code when the system doesn't know it. */
export function countryName(code: string) {
  try {
    return countryNames.of(code) ?? code
  } catch {
    return code
  }
}

/** A country's flag from its ISO code, or nothing when the code isn't two letters. */
export function countryFlag(code: string) {
  if (!/^[A-Z]{2}$/.test(code)) return ''
  return String.fromCodePoint(0x1f1a5 + code.charCodeAt(0), 0x1f1a5 + code.charCodeAt(1))
}

/**
 * Where a purchase was made, in one line: "Sunnyvale, CA" at home in the US, "Zürich,
 * Switzerland" abroad. Rust chose the place; this only words it.
 */
export function placeLabel(place: Place | undefined) {
  if (!place) return ''
  if (place.country === 'US')
    return [place.city, place.region].filter(Boolean).join(', ') || 'United States'
  return [place.city, countryName(place.country)].filter(Boolean).join(', ')
}

/** Where the place came from, for the inspector. */
export function placeSource(place: Place) {
  switch (place.source) {
    case 'issuer':
      return 'From the card issuer’s records'
    case 'descriptor':
      return 'From the bank’s descriptor'
    default:
      return 'From the bank’s location'
  }
}

/** A street address with its city, as Maps reads one: "1 Main St, Sunnyvale, CA 94087". */
function fullAddress(street: string, place: Place | undefined) {
  if (!place) return street
  const locality =
    place.country === 'US'
      ? [place.city, [place.region, place.postalCode].filter(Boolean).join(' ')]
      : [place.city, place.postalCode, countryName(place.country)]
  return [street, ...locality].filter(Boolean).join(', ')
}

/** A Google Maps search (Maps URLs, no key): a business name with its address opens its card. */
export function googleMapsUrl(query: string) {
  return `https://www.google.com/maps/search/?api=1&query=${encodeURIComponent(query)}`
}

/**
 * A Google Maps link: the business by name at its street address when the records give one,
 * which opens its place card; a pin at the spot when Rust pinned it without an address;
 * otherwise a search for the name in its place. A city's centre is never offered as where the
 * purchase was.
 */
export function placeMapsUrl(name: string, place: Place | undefined, address?: string) {
  const street = address || place?.address
  if (street) return googleMapsUrl(`${name}, ${fullAddress(street, place)}`)
  if (place?.precision === 'exact' && place.lat != null && place.lon != null)
    return googleMapsUrl(`${place.lat},${place.lon}`)
  const query = [name, placeLabel(place)].filter(Boolean).join(', ')
  return query ? googleMapsUrl(query) : undefined
}
