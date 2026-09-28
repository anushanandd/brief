import type { MarketNewsArticle } from './schema'

export type NewsGroup = { article: MarketNewsArticle; related: MarketNewsArticle[] }
const tokens = (headline: string) =>
  new Set(
    headline
      .toLowerCase()
      .match(/[a-z0-9]+/g)
      ?.filter(
        (word) => !['the', 'a', 'an', 'and', 'of', 'to', 'in', 'for', 'on', 'with'].includes(word),
      ) ?? [],
  )

const direction = (headline: string) =>
  headline
    .toLowerCase()
    .match(/\b(?:not|no|raises?|cuts?|beats?|misses?|upgrades?|downgrades?|approves?|rejects?)\b/g)
    ?.join(' ') ?? ''

function sameStory(left: MarketNewsArticle, right: MarketNewsArticle) {
  if (left.url === right.url) return true
  if (Math.abs(Date.parse(left.createdAt) - Date.parse(right.createdAt)) > 48 * 3600000)
    return false
  // Different figures often identify a different report or update.
  if (
    JSON.stringify(left.headline.match(/\d+(?:\.\d+)?/g)) !==
    JSON.stringify(right.headline.match(/\d+(?:\.\d+)?/g))
  )
    return false
  if (direction(left.headline) !== direction(right.headline)) return false
  const a = tokens(left.headline),
    b = tokens(right.headline)
  const overlap = [...a].filter((word) => b.has(word)).length
  return overlap >= 3 && overlap / new Set([...a, ...b]).size >= 0.75
}

export function rankHoldingNews(articles: MarketNewsArticle[], ticker: string): NewsGroup[] {
  const ranked = articles
    .filter(
      (article) =>
        article.symbols.includes(ticker) && Number.isFinite(Date.parse(article.createdAt)),
    )
    .toSorted((a, b) => b.createdAt.localeCompare(a.createdAt) || a.url.localeCompare(b.url))
  const groups: NewsGroup[] = []
  for (const article of ranked) {
    const group = groups.find((candidate) => sameStory(candidate.article, article))
    if (!group) groups.push({ article, related: [] })
    else if (
      group.article.url !== article.url &&
      !group.related.some((other) => other.url === article.url)
    )
      group.related.push(article)
  }
  return groups
}
