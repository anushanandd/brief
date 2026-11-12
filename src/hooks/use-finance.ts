import { replaceEqualDeep, type QueryClient } from '@tanstack/react-query'
import { createContext, useContext } from 'react'

import type { FinanceSnapshot } from '../lib/schema'
import type { FinanceState } from './finance-provider'

export const financeQueryKey = ['finance-snapshot'] as const
export const FinanceContext = createContext<FinanceState | null>(null)

export function useFinance() {
  const finance = useContext(FinanceContext)
  if (!finance) throw new Error('useFinance requires FinanceProvider')
  return finance
}

/**
 * Which snapshot to hold when another arrives. Rust advances `revision` with each refresh,
 * restore, and erase, so one with a lower revision was read before the held one and is
 * dropped. At an equal revision the arrival wins: small edits keep the revision, and a later
 * read carries the later edit or derived view (a new day's insights, say). A snapshot without
 * a revision, such as the browser preview's, can't be ordered and is taken as it comes, and
 * so is anything replacing a recovery view, which stands for state that couldn't be read.
 */
export function newerSnapshot<T extends Ordered>(held: T | undefined, incoming: T): T {
  return held?.revision != null &&
    incoming.revision != null &&
    !held.recovery &&
    incoming.revision < held.revision
    ? held
    : incoming
}

type Ordered = Pick<FinanceSnapshot, 'revision' | 'recovery'>
export type SnapshotIdentity = Pick<FinanceSnapshot, 'revision' | 'updatedAt' | 'insights'>
const ordered = (value: unknown): value is Ordered => typeof value === 'object' && value !== null

/**
 * The finance query's structural sharing: keeps the held snapshot over an older one, so a slow
 * read finishing late, from a refetch or any writer, never rolls the view back.
 */
export function shareFinanceSnapshot(held: unknown, incoming: unknown) {
  if (ordered(held) && ordered(incoming) && newerSnapshot(held, incoming) === held) return held
  return replaceEqualDeep(held, incoming)
}

/**
 * What identifies a snapshot's content, for the key of every query Rust derives from it.
 * `revision` advances with each refresh, restore, and erase; small edits (annotations,
 * settings) keep it, but Rust derives the view again for every change to saved data, so the
 * insights' time tells one edit's view from the next. A snapshot without insights (a recovery
 * view) falls back to its save time.
 *
 * Market reads (live prices, chart previews, today's move) key on the refresh instead, since
 * an edit changes none of their inputs and refetching them costs a network round trip.
 */
export function snapshotKey(data: SnapshotIdentity | undefined) {
  return [data?.revision ?? null, data?.insights?.computedAt ?? data?.updatedAt ?? null] as const
}

export type HoldingsIdentity = Pick<FinanceSnapshot, 'revision' | 'holdings'>

/**
 * What identifies the holdings, for the key of every query Rust derives from them and the
 * market alone (sector exposure). A refresh advances `revision`; among edits only a
 * manual account or a closed date changes the holdings Rust reports, so a note, tag, or
 * category leaves these queries, and the work behind them, alone.
 */
export function holdingsKey(data: HoldingsIdentity | undefined) {
  return [data?.revision ?? null, data?.holdings ?? null] as const
}

/** Every writer of the snapshot goes through here: edits, refreshes, Wallet, and restores. */
export function setFinanceSnapshot(client: QueryClient, snapshot: FinanceSnapshot) {
  client.setQueryData<FinanceSnapshot>(financeQueryKey, (held) => newerSnapshot(held, snapshot))
}
