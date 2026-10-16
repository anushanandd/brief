/** Company news and SEC filings for held stocks. */
import { z } from 'zod'

export const newsTagSchema = z.enum([
  'earnings',
  'guidance',
  'product',
  'deal',
  'legal',
  'leadership',
  'capital',
  'explainer',
])
export type NewsTag = z.infer<typeof newsTagSchema>
export const marketNewsSchema = z.array(
  z.object({
    headline: z.string(),
    summary: z.string(),
    source: z.string(),
    url: z.string().url().startsWith('https://'),
    createdAt: z.string().datetime({ offset: true }),
    symbols: z.array(z.string()),
    scope: z.enum(['company', 'market']),
    tags: z.array(newsTagSchema),
    // How much the story is about the requested company, 0 to 100.
    relevance: z.number().int().min(0).max(100),
  }),
)
export type MarketNewsArticle = z.infer<typeof marketNewsSchema>[number]
export const secFilingSchema = z.object({
  form: z.string(),
  filedAt: z.string().datetime({ offset: true }),
  url: z.string().url().startsWith('https://www.sec.gov/'),
  items: z.array(z.object({ code: z.string(), label: z.string() })),
  title: z.string(),
})
export type SecFiling = z.infer<typeof secFilingSchema>
export const marketNewsResultSchema = z.object({
  articles: marketNewsSchema,
  savedAt: z.string().datetime({ offset: true }).nullable(),
  warning: z.string().nullable(),
  canRefresh: z.boolean(),
  company: z.string().nullable(),
  filings: z.array(secFilingSchema),
  filingsSavedAt: z.string().datetime({ offset: true }).nullable(),
  filingsWarning: z.string().nullable(),
})
export type MarketNewsResult = z.infer<typeof marketNewsResultSchema>
