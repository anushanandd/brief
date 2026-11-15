import type { IntegrationStatus } from './schema'

/**
 * Every data provider Brief reads from, in display order, with where Keychain's saved status
 * lives for it. Account ids from a provider start with its id.
 */
export const providerCatalog = [
  { id: 'plaid', name: 'Plaid', status: 'plaid' },
  { id: 'snaptrade', name: 'SnapTrade', status: 'snaptrade' },
  { id: 'alpaca', name: 'Alpaca', status: 'alpaca' },
  { id: 'alphavantage', name: 'Alpha Vantage', status: 'alphaVantage' },
  { id: 'sec', name: 'SEC EDGAR', status: 'sec' },
] as const satisfies ReadonlyArray<{ id: string; name: string; status: keyof IntegrationStatus }>

export type ProviderId = (typeof providerCatalog)[number]['id']
