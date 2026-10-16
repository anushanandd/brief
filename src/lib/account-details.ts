/** A typed website as a secure link: `bank.com` reads as `https://bank.com`. */
export function secureWebsite(value: string) {
  const website = value.trim()
  if (!website) return undefined
  return /^[a-z][a-z0-9+.-]*:\/\//i.test(website) ? website : `https://${website}`
}

/** A typed rate in percent between 0 and 100, or nothing when it isn't one. */
export function percent(value: string) {
  const rate = Number(value.replace('%', '').trim())
  return value.trim() && Number.isFinite(rate) && rate >= 0 && rate <= 100 ? rate : undefined
}
