import { z } from 'zod'

/** Which providers have credentials saved in Keychain. */
export const integrationStatusSchema = z.object({
  plaid: z.boolean(),
  snaptrade: z.boolean(),
  alpaca: z.boolean(),
  alphaVantage: z.boolean(),
  sec: z.boolean(),
})
export type IntegrationStatus = z.infer<typeof integrationStatusSchema>

const connectionProviderSchema = z.enum(['plaid', 'plaid-investments', 'snaptrade'])
/** A connection's provider, or adding statements to a saved Plaid bank connection. */
const linkProviderSchema = z.enum([...connectionProviderSchema.options, 'plaid-statements'])

/** A provider link the browser is completing. */
export const providerLinkSessionSchema = z.object({
  provider: linkProviderSchema,
  sessionId: z.string(),
  url: z.string(),
})
export type ProviderLinkSession = z.infer<typeof providerLinkSessionSchema>

export const providerLinkStatusSchema = z.object({ status: z.enum(['pending', 'connected']) })
export type ProviderLinkStatus = z.infer<typeof providerLinkStatusSchema>

/** A connected institution or brokerage login. */
export const providerConnectionSchema = z.object({
  itemId: z.string(),
  name: z.string(),
  provider: connectionProviderSchema,
  error: z.string().nullable(),
  /** For a Plaid bank connection, whether its statements were added. */
  statements: z.boolean().optional(),
})
export type ProviderConnection = z.infer<typeof providerConnectionSchema>

/** What a backup file holds, read before restoring it. */
export const backupInfoSchema = z.object({
  updatedAt: z.string(),
  revision: z.number(),
  schemaVersion: z.number(),
  accounts: z.number(),
  transactions: z.number(),
  holdings: z.number(),
  documents: z.number().default(0),
  /** Whether the documents' originals open here, or need the backup's passphrase. */
  documentKey: z.enum(['thisMac', 'passphrase', 'otherMac', 'none']).default('none'),
})
export type BackupInfo = z.infer<typeof backupInfoSchema>

export const foundationModelStatusSchema = z.object({
  state: z.enum(['available', 'unavailable', 'disabled', 'notReady']),
  message: z.string(),
})
export type FoundationModelStatus = z.infer<typeof foundationModelStatusSchema>

export const loginItemStateSchema = z.enum([
  'enabled',
  'disabled',
  'requires-approval',
  'unavailable',
])
export type LoginItemState = z.infer<typeof loginItemStateSchema>

/** A bank statement PDF saved from Plaid Statements, without its contents. */
export const bankStatementSchema = z.object({
  id: z.string(),
  itemId: z.string(),
  accountId: z.string(),
  accountName: z.string(),
  year: z.number().int(),
  month: z.number().int().min(1).max(12),
  datePosted: z.string().optional(),
  fetchedAt: z.string(),
  bytes: z.number().int().nonnegative(),
})
export type BankStatement = z.infer<typeof bankStatementSchema>

/** What one statements check found. */
export const statementSyncSchema = z.object({
  available: z.number().int().nonnegative(),
  saved: z.number().int().nonnegative(),
  failures: z.array(z.string()),
})
export type StatementSync = z.infer<typeof statementSyncSchema>
