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

const linkProviderSchema = z.enum(['plaid', 'plaid-investments', 'snaptrade'])

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
  provider: linkProviderSchema,
  error: z.string().nullable(),
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
