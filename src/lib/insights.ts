export function briefMerchant(name: string) {
  const words = name.trim().split(/\s+/)
  const repeated = words.findIndex(
    (word, index) => index > 0 && word.toLowerCase() === words[0]?.toLowerCase(),
  )
  const clean = (repeated > 0 ? words.slice(0, repeated) : words)
    .join(' ')
    .replace(/\s+(?:sol|llc|inc|corp|ltd)\.?$/i, '')
  return clean.slice(0, 100)
}
