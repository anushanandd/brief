import { describe, expect, it } from 'vitest'

import { activityPlace, type ActivityItem } from './activity'
import { countryFlag, placeLabel, placeMapsUrl, placeSource, type Place } from './places'

const zurich: Place = {
  city: 'Zürich',
  region: 'ZH',
  country: 'CH',
  lat: 47.367,
  lon: 8.55,
  source: 'descriptor',
  precision: 'city',
}

describe('places', () => {
  it('words a place at home and abroad', () => {
    expect(
      placeLabel({
        city: 'Sunnyvale',
        region: 'CA',
        country: 'US',
        source: 'bank',
        precision: 'city',
      }),
    ).toBe('Sunnyvale, CA')
    expect(
      placeLabel({ region: 'CA', country: 'US', source: 'descriptor', precision: 'region' }),
    ).toBe('CA')
    expect(placeLabel(zurich)).toBe('Zürich, Switzerland')
    expect(placeSource(zurich)).toBe('From the bank’s descriptor')
  })

  it('flags a country by its code, and nothing without one', () => {
    expect(countryFlag('CH')).toBe('🇨🇭')
    expect(countryFlag('us')).toBe('')
    expect(countryFlag('XYZ')).toBe('')
  })

  it('opens the business by name at its address, a pinned spot, or else a search', () => {
    const google = 'https://www.google.com/maps/search/?api=1&query='
    expect(
      placeMapsUrl('Example Cafe', {
        address: '1 SAMPLE ST',
        postalCode: '94087',
        city: 'Sunnyvale',
        region: 'CA',
        country: 'US',
        lat: 37.37,
        lon: -122.04,
        source: 'issuer',
        precision: 'street',
      }),
    ).toBe(`${google}Example%20Cafe%2C%201%20SAMPLE%20ST%2C%20Sunnyvale%2C%20CA%2094087`)
    expect(placeMapsUrl('Example Cafe', zurich, 'Bahnhofstrasse 1')).toBe(
      `${google}Example%20Cafe%2C%20Bahnhofstrasse%201%2C%20Z%C3%BCrich%2C%20Switzerland`,
    )
    expect(placeMapsUrl('Example Cafe', { ...zurich, precision: 'exact' })).toBe(
      `${google}47.367%2C8.55`,
    )
    // A city's centre isn't where the purchase was.
    expect(placeMapsUrl('Example Cafe', zurich)).toBe(
      `${google}Example%20Cafe%2C%20Z%C3%BCrich%2C%20Switzerland`,
    )
    expect(
      placeMapsUrl('Example Cafe', {
        region: 'CA',
        country: 'US',
        source: 'descriptor',
        precision: 'region',
      }),
    ).toBe(`${google}Example%20Cafe%2C%20CA`)
  })

  it('shows the resolved place in the ledger before the bank’s location', () => {
    const activity = {
      id: 'spending:a',
      kind: 'spending',
      category: 'Dining',
      title: 'Example Cafe',
      detail: '',
      date: '2026-10-01',
      amount: -12,
      paymentChannel: 'in store',
      // The bank reported a corporate address; the descriptor named the store's city.
      location: { city: 'Oregon', region: 'OH' },
      place: zurich,
    } as ActivityItem
    expect(activityPlace(activity)).toMatchObject({ kind: 'place', primary: 'Zürich, Switzerland' })
  })

  it('shows the issuer’s street, and a remote purchase as online wherever it was billed', () => {
    const base = {
      id: 'spending:a',
      kind: 'spending',
      category: 'Dining',
      title: 'Example Cafe',
      detail: '',
      date: '2026-10-01',
      amount: -12,
      paymentChannel: 'in store',
    } as ActivityItem
    expect(
      activityPlace({
        ...base,
        presence: 'inPerson',
        place: {
          address: '1 SAMPLE ST',
          city: 'Sunnyvale',
          region: 'CA',
          country: 'US',
          source: 'issuer',
          precision: 'street',
        },
      }),
    ).toMatchObject({ kind: 'place', primary: 'Sunnyvale, CA', secondary: '1 Sample ST' })
    expect(
      activityPlace({
        ...base,
        presence: 'remote',
        location: { city: 'San Francisco', region: 'CA' },
        website: 'https://www.example.com',
      }),
    ).toMatchObject({ kind: 'online', primary: 'Online', secondary: 'example.com' })
  })
})
