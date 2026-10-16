import { expect, it } from 'vitest'

import { addDays, daysBetween, validDate } from './dates'

it('accepts only real calendar dates', () => {
  expect(validDate('2024-02-29')).toBe('2024-02-29')
  expect(validDate('2000-02-29')).toBe('2000-02-29')
  expect(validDate('1900-02-29')).toBeUndefined()
  expect(validDate('2026-02-29')).toBeUndefined()
  expect(validDate('2026-04-31')).toBeUndefined()
  expect(validDate('2026-12-31')).toBe('2026-12-31')
  expect(validDate('2026-13-01')).toBeUndefined()
  expect(validDate('2026-00-10')).toBeUndefined()
  expect(validDate('2026-1-01')).toBeUndefined()
  expect(validDate(20260101)).toBeUndefined()
})

it('counts calendar days across month, year, and daylight-saving boundaries', () => {
  expect(addDays('2026-02-28', 1)).toBe('2026-03-01')
  expect(addDays('2026-01-01', -1)).toBe('2025-12-31')
  expect(addDays('2026-03-07', 2)).toBe('2026-03-09')
  expect(daysBetween('2026-03-07', '2026-03-09')).toBe(2)
  expect(daysBetween('2026-09-10', '2026-09-03')).toBe(-7)
})
