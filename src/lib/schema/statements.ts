/** Statement files chosen for import: their preview before, and their summary after. */
import { z } from 'zod'

const statementCheckSchema = z.object({
  status: z.enum(['verified', 'partial', 'unverified', 'failed']),
  totalMatches: z.boolean().nullable(),
  balanceRows: z.number().int().nonnegative(),
  balanceExplained: z.number().int().nonnegative(),
})

const statementCountsSchema = z.object({
  added: z.number().int().nonnegative(),
  covered: z.number().int().nonnegative(),
  repeated: z.number().int().nonnegative(),
  pending: z.number().int().nonnegative(),
})

export const statementPreviewSchema = z.object({
  draftId: z.string(),
  fileName: z.string(),
  label: z.string().nullable(),
  firstDate: z.string(),
  lastDate: z.string(),
  rows: z.number().int().nonnegative(),
  trades: z.number().int().nonnegative(),
  moneyIn: z.number(),
  moneyOut: z.number(),
  check: statementCheckSchema,
  datesAssumed: z.boolean(),
  suggestedAccountId: z.string().nullable(),
  importedAccountId: z.string().nullable(),
  accounts: z.array(
    z.object({
      accountId: z.string(),
      counts: statementCountsSchema,
      providerStart: z.string().nullable(),
    }),
  ),
})

export const statementSummarySchema = z.object({
  id: z.string(),
  fileName: z.string(),
  accountId: z.string(),
  importedAt: z.string(),
  firstDate: z.string().nullable(),
  lastDate: z.string().nullable(),
  rows: z.number().int().nonnegative(),
  counts: statementCountsSchema,
  check: statementCheckSchema.nullable(),
})

export type StatementCheck = z.infer<typeof statementCheckSchema>
export type StatementPreview = z.infer<typeof statementPreviewSchema>
export type StatementSummary = z.infer<typeof statementSummarySchema>
