import { expect, it } from 'vitest'

import { percent, secureWebsite } from './account-details'

it('reads a typed website as a secure link and leaves other schemes for Rust to refuse', () => {
  expect(secureWebsite(' bank.example/savings ')).toBe('https://bank.example/savings')
  expect(secureWebsite('https://bank.example')).toBe('https://bank.example')
  expect(secureWebsite('http://bank.example')).toBe('http://bank.example')
  expect(secureWebsite('  ')).toBeUndefined()
})

it('reads a rate in percent and nothing outside 0–100', () => {
  expect(percent('4.15%')).toBe(4.15)
  expect(percent('0')).toBe(0)
  expect(percent('')).toBeUndefined()
  expect(percent('120')).toBeUndefined()
  expect(percent('high')).toBeUndefined()
})
