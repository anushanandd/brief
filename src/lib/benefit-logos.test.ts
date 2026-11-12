// @vitest-environment jsdom
import { beforeEach, expect, it } from 'vitest'

import { benefitLogoUrls, warmBenefitLogos } from './logos'

beforeEach(() => localStorage.clear())

it("orders a benefit's logos, and loads none with external logos off", () => {
  const logo = {
    logoDomain: 'examplebrand.com',
    logoUrl: 'https://plaid-merchant-logos.plaid.com/examplebrand.png',
    logoBrand: 'examplebrand',
  }
  const hosts = benefitLogoUrls(logo, true).map((url) => new URL(url).host + new URL(url).pathname)
  expect(hosts).toEqual([
    'img.logo.dev/examplebrand.com',
    'plaid-merchant-logos.plaid.com/examplebrand.png',
    'img.logo.dev/name/examplebrand',
  ])
  expect(benefitLogoUrls(logo, false)).toEqual([])
  expect(benefitLogoUrls({ logoUrl: 'https://elsewhere.example/logo.png' }, true)).toEqual([])
})

it("warms every catalog brand's logo by name a few at a time, at most once a day", async () => {
  const day = 24 * 60 * 60 * 1000
  const loaded: string[] = []
  const pending: Array<() => void> = []
  const all = ['brand a', 'brand b', 'brand c', 'brand d', 'brand e']
  let asked = 0
  const run = (now: number, external = true, brands: readonly string[] = all) =>
    warmBenefitLogos({
      external,
      brands: async () => {
        asked += 1
        return brands
      },
      now,
      load: async (src) => {
        loaded.push(decodeURIComponent(new URL(src).pathname))
      },
      schedule: (step) => pending.push(step),
    })
  const next = async () => {
    pending.shift()?.()
    await new Promise((resolve) => setTimeout(resolve, 0))
  }

  expect(await run(day, false)).toBe(false)
  expect(asked).toBe(0)
  // With no catalog, the day isn't spent, but this session doesn't ask again.
  expect(await run(day, true, [])).toBe(false)
  expect(await run(day)).toBe(false)
  expect(asked).toBe(1)
  expect(pending).toEqual([])

  expect(await run(2 * day)).toBe(true)
  expect(loaded).toEqual([])
  await next()
  expect(loaded).toHaveLength(4)
  await next()
  await next()
  expect(loaded).toEqual(all.map((brand) => `/name/${brand}`))
  expect(pending).toEqual([])

  expect(await run(2 * day + day / 2)).toBe(false)
  expect(await run(3 * day)).toBe(true)
})
