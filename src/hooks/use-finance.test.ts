import { QueryClient } from '@tanstack/react-query'
import { expect, it } from 'vitest'

import { summaryKey } from '../components/investment-summary-card'
import { emptySnapshot, seedSnapshot } from '../data/fixtures/records'
import type { FinanceSnapshot } from '../lib/schema'
import {
  financeQueryKey,
  holdingsKey,
  setFinanceSnapshot,
  shareFinanceSnapshot,
  snapshotKey,
} from './use-finance'

const at = (revision: number | undefined, note: string): FinanceSnapshot => ({
  ...emptySnapshot(),
  revision,
  syncWarnings: [note],
})
const held = (client: QueryClient) => client.getQueryData<FinanceSnapshot>(financeQueryKey)

it('keeps the newer snapshot when an older response lands late', () => {
  const client = new QueryClient()
  setFinanceSnapshot(client, at(4, 'refreshed'))
  setFinanceSnapshot(client, at(3, 'late edit'))
  expect(held(client)?.syncWarnings).toEqual(['refreshed'])
  setFinanceSnapshot(client, at(5, 'next refresh'))
  expect(held(client)?.revision).toBe(5)
})

it('takes the arrival at an equal revision, and any snapshot it cannot order', () => {
  const client = new QueryClient()
  setFinanceSnapshot(client, at(4, 'before the note'))
  // Small edits keep the revision; the later answer carries the later edit.
  setFinanceSnapshot(client, at(4, 'with the note'))
  expect(held(client)?.syncWarnings).toEqual(['with the note'])
  setFinanceSnapshot(client, at(undefined, 'preview'))
  expect(held(client)?.syncWarnings).toEqual(['preview'])
})

it('replaces a recovery view whatever its revision', () => {
  const client = new QueryClient()
  setFinanceSnapshot(client, {
    ...at(9, 'unreadable'),
    recovery: { message: 'Synthetic recovery', canRestore: true },
  })
  setFinanceSnapshot(client, at(2, 'recovered'))
  expect(held(client)?.syncWarnings).toEqual(['recovered'])
})

it("guards the query's own refetches the same way", () => {
  const newer = at(4, 'newer')
  expect(shareFinanceSnapshot(newer, at(3, 'older'))).toBe(newer)
  expect(shareFinanceSnapshot(newer, at(5, 'next'))).toEqual(at(5, 'next'))
  expect(shareFinanceSnapshot(undefined, newer)).toBe(newer)
})

it('keys derived queries by content, so an edit at the same revision reads again', () => {
  const seed = seedSnapshot()
  if (!seed.insights) throw new Error('The seed has insights')
  const before = { ...seed, revision: 4 }
  // An annotation edit keeps the revision; Rust derives the view again.
  const after = { ...before, insights: { ...seed.insights, computedAt: '2026-03-02T10:00:05Z' } }
  expect(snapshotKey(after)).not.toEqual(snapshotKey(before))
  expect(snapshotKey({ ...before })).toEqual(snapshotKey(before))
  expect(snapshotKey({ ...before, revision: 5 })).not.toEqual(snapshotKey(before))
  expect(snapshotKey(undefined)).toEqual([null, null])
})

/** The keys of queries Rust derives from the snapshot: holdings-only ones, and ledger reads. */
const keys = (data: FinanceSnapshot) => ({
  summary: summaryKey('2026-01-01', undefined, data),
  sectors: holdingsKey(data),
  health: ['data-health', ...snapshotKey(data)],
  ledger: ['ledger-query', 'pin', {}, ...snapshotKey(data)],
})

it('keeps holdings-only keys through an annotation edit while ledger reads refetch', () => {
  const seed = seedSnapshot()
  if (!seed.insights) throw new Error('The seed has insights')
  const before: FinanceSnapshot = { ...seed, revision: 4 }
  const spend = seed.transactions.find((t) => t.classification?.mark !== 'dividend')
  if (!spend?.classification) throw new Error('The seed has a classified spend')
  // A note: Rust derives the view again at the same revision, and nothing the summary or
  // sectors read changes.
  const noted: FinanceSnapshot = {
    ...before,
    insights: { ...seed.insights, computedAt: '2026-03-02T10:00:05Z' },
    transactions: seed.transactions.map((t) => (t === spend ? { ...t, note: 'Synthetic' } : t)),
  }
  const [was, now] = [keys(before), keys(noted)]
  expect(now.summary).toEqual(was.summary)
  expect(now.sectors).toEqual(was.sectors)
  expect(now.health).not.toEqual(was.health)
  expect(now.ledger).not.toEqual(was.ledger)

  // A category that makes the spend a dividend reaches the summary, and only the summary.
  const classification = { ...spend.classification, mark: 'dividend' as const }
  const dividend: FinanceSnapshot = {
    ...noted,
    transactions: seed.transactions.map((t) => (t === spend ? { ...t, classification } : t)),
  }
  expect(keys(dividend).summary).not.toEqual(was.summary)
  expect(keys(dividend).sectors).toEqual(was.sectors)
  // A refresh moves all of them.
  expect(keys({ ...before, revision: 5 }).sectors).not.toEqual(was.sectors)
})
