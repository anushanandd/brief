import { useMemo } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { SpendingOwner } from '../lib/schema'
import { type SpendingPeriod, spendingOwnerOptions, spendingPeriodOptions } from '../lib/spending'

export type SpendingComparison = 'previous' | 'typical'
export type SpendingBreakdown = 'categories' | 'places'

/**
 * Spending's page commands: a period per key, stepping through periods, whose spending counts,
 * clearing focus, what categories compare against, and whether the breakdown is by place.
 */
export function useSpendingActions({
  accounts,
  account,
  chooseAccount,
  hasStatements,
  hasOthers,
  owner,
  setOwner,
  focus,
  period,
  periodOffset,
  hasEarlier,
  choosePeriod,
  setPeriodOffset,
  clearFocus,
  comparison,
  canCompareTypical,
  setComparison,
  breakdown,
  setBreakdown,
}: {
  /** Every account first, then each account with spending, the saved card leading. */
  accounts: Array<{ value: string; label: string }>
  /** The choice in view. */
  account: string
  chooseAccount: (account: string) => void
  hasStatements: boolean
  /** A purchase is someone else's, so whose spending counts is a choice. */
  hasOthers: boolean
  owner: SpendingOwner
  setOwner: (owner: SpendingOwner) => void
  focus: string | null
  period: SpendingPeriod
  periodOffset: number
  /** The saved history reaches a period before the one shown. */
  hasEarlier: boolean
  /** Shows a period from its latest instance. */
  choosePeriod: (period: SpendingPeriod) => void
  setPeriodOffset: (update: (current: number) => number) => void
  clearFocus: () => void
  comparison: SpendingComparison
  /** Earlier periods cover this one, so an average exists. */
  canCompareTypical: boolean
  setComparison: (comparison: SpendingComparison) => void
  breakdown: SpendingBreakdown
  setBreakdown: (breakdown: SpendingBreakdown) => void
}) {
  const actions = useMemo<AppAction[]>(
    () => [
      // 0 is every account and 1–9 each one, in the order the account menu lists them.
      ...(accounts.length > 2 ? accounts : []).slice(0, 10).map(({ value, label }, index) => ({
        id: `spending.account.${value}`,
        title: `Show spending in ${label === 'All accounts' ? 'all accounts' : label}`,
        search: 'spending account card cards apple amex all accounts choose switch',
        group: 'View',
        scope: 'page' as const,
        bindings: [{ sequence: [String(index)], label: String(index) }],
        disabled: account === value,
        disabledReason: `Already showing ${label}`,
        run: () => chooseAccount(value),
      })),
      ...spendingPeriodOptions.map(({ key, label, value }) => ({
        disabled: value === 'statement' && !hasStatements,
        disabledReason:
          value === 'statement' && !hasStatements
            ? 'Statements belong to the spending card set in Settings'
            : undefined,
        id: `spending.period.${String(value)}`,
        title: `Show ${label} spending`,
        search: `spending period range ${label}`,
        group: 'View',
        scope: 'page' as const,
        bindings: [{ sequence: [key], label: key.toUpperCase() }],
        run: () => choosePeriod(value),
      })),
      ...spendingOwnerOptions.map(({ value, label }) => ({
        disabled: !hasOthers || owner === value,
        disabledReason: !hasOthers
          ? 'No purchase is marked for someone else'
          : owner === value
            ? `Already showing ${label.toLowerCase()}`
            : undefined,
        id: `spending.for.${value}`,
        title: `Show ${label === 'All spending' ? 'all spending' : `spending ${label.toLowerCase()}`}`,
        search: 'spending whose for me other someone else everyone all owner',
        group: 'View',
        scope: 'page' as const,
        run: () => setOwner(value),
      })),
      {
        id: 'spending.for.next',
        title: 'Switch whose spending is shown',
        search: 'spending whose for me other someone else everyone all owner cycle',
        group: 'View',
        scope: 'page',
        bindings: [{ sequence: ['f'], label: 'F' }],
        disabled: !hasOthers,
        disabledReason: hasOthers ? undefined : 'No purchase is marked for someone else',
        run: () => {
          const index = spendingOwnerOptions.findIndex(({ value }) => value === owner)
          setOwner(spendingOwnerOptions[(index + 1) % spendingOwnerOptions.length].value)
        },
      },
      {
        id: 'spending.focus.clear',
        title: 'Clear category focus',
        search: 'spending category focus filter clear all categories',
        group: 'View',
        scope: 'page',
        // Esc belongs to focus only while a category has it.
        bindings: focus ? [{ sequence: ['Escape'], label: 'Esc' }] : undefined,
        disabled: !focus,
        disabledReason: focus ? undefined : 'No category is focused',
        run: clearFocus,
      },
      {
        id: 'spending.breakdown.toggle',
        title: breakdown === 'places' ? 'Show spending by category' : 'Show spending by place',
        search: 'spending places cities countries where location categories breakdown',
        group: 'View',
        scope: 'page',
        // B starts an account jump, so this takes ⇧B.
        bindings: [{ sequence: ['B'], label: '⇧B' }],
        run: () => setBreakdown(breakdown === 'places' ? 'categories' : 'places'),
      },
      {
        id: 'spending.compare.previous',
        title: 'Compare categories with the last period',
        search: 'spending categories compare last statement previous period',
        group: 'View',
        scope: 'page',
        disabled: period === 0 || comparison === 'previous',
        disabledReason:
          period === 0
            ? 'All time has no earlier period'
            : comparison === 'previous'
              ? 'Already comparing with the last period'
              : undefined,
        run: () => setComparison('previous'),
      },
      {
        id: 'spending.compare.typical',
        title: 'Compare categories with the average',
        search: 'spending categories compare average usual typical',
        group: 'View',
        scope: 'page',
        disabled: !canCompareTypical || comparison === 'typical',
        disabledReason: !canCompareTypical
          ? 'Not enough earlier periods for an average'
          : comparison === 'typical'
            ? 'Already comparing with the average'
            : undefined,
        run: () => setComparison('typical'),
      },
      {
        id: 'spending.period.previous',
        title: 'Previous spending period',
        group: 'View',
        scope: 'page',
        bindings: [
          { sequence: ['ArrowLeft'], label: '←' },
          { sequence: ['['], label: '[' },
        ],
        disabled: period === 0 || !hasEarlier,
        disabledReason:
          period === 0
            ? 'All time has no earlier period'
            : hasEarlier
              ? undefined
              : 'No earlier period in the saved history',
        run: () => setPeriodOffset((current) => current - 1),
      },
      {
        id: 'spending.period.next',
        title: 'Next spending period',
        group: 'View',
        scope: 'page',
        bindings: [
          { sequence: ['ArrowRight'], label: '→' },
          { sequence: [']'], label: ']' },
        ],
        disabled: periodOffset === 0,
        disabledReason: periodOffset === 0 ? 'Already showing the latest period' : undefined,
        run: () => setPeriodOffset((current) => Math.min(0, current + 1)),
      },
    ],
    [
      account,
      accounts,
      breakdown,
      canCompareTypical,
      chooseAccount,
      choosePeriod,
      clearFocus,
      comparison,
      focus,
      hasOthers,
      hasEarlier,
      hasStatements,
      owner,
      period,
      setOwner,
      periodOffset,
      setBreakdown,
      setComparison,
      setPeriodOffset,
    ],
  )
  useRegisterActions(actions)
}
