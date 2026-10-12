import { formatDay } from './format'
import type { StatementCheck, StatementPreview } from './schema'

type Counts = StatementPreview['accounts'][number]['counts']

/** The provider whose history a statement fills in front of, from the account id. */
export function statementProvider(accountId: string) {
  if (accountId.startsWith('plaid:')) return 'Plaid'
  if (accountId.startsWith('snaptrade:')) return 'SnapTrade'
  return 'the provider'
}

/** What Rust's check of a statement against its own total and running balances found. */
export function statementCheckLabel(check: StatementCheck) {
  if (check.status === 'failed') return 'Total doesn’t match the rows'
  if (check.status === 'unverified') return 'No total or balances to check'
  if (check.status === 'partial')
    return `Balances match for ${check.balanceExplained} of ${check.balanceRows} rows`
  if (check.totalMatches && check.balanceRows) return 'Total and balances match'
  return check.totalMatches ? 'Total matches' : 'Balances match'
}

export function statementDateRange(first: string | null, last: string | null) {
  if (!first || !last) return undefined
  return first === last ? formatDay(first) : `${formatDay(first)} – ${formatDay(last)}`
}

/** What importing adds to an account, beside what its provider and earlier statements cover. */
export function statementCountsLabel(
  counts: Counts,
  accountId: string,
  providerStart?: string | null,
) {
  const provider = statementProvider(accountId)
  const parts = [
    counts.added
      ? `Adds ${counts.added} ${counts.added === 1 ? 'activity' : 'activities'}${
          providerStart ? ` before ${formatDay(providerStart)}` : ''
        }`
      : counts.covered
        ? `Adds nothing; ${provider} covers these dates`
        : 'Adds nothing new',
  ]
  if (counts.added && counts.covered) parts.push(`${counts.covered} already from ${provider}`)
  if (counts.repeated) parts.push(`${counts.repeated} in earlier statements`)
  if (counts.pending) parts.push(`${counts.pending} pending skipped`)
  return parts.join(' · ')
}
