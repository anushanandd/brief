import { useMemo } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import type { SpendingOwner } from '../lib/schema'
import { type SpendingPeriod, spendingOwnerOptions, spendingPeriodOptions } from '../lib/spending'

export type SpendingComparison = 'previous' | 'typical'

/**
 * Spending's page commands: a period per key, stepping through periods, whose spending counts,
 * clearing focus, and what categories compare against.
 */
export function useSpendingActions({
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
}: {
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
}) {
  const actions = useMemo<AppAction[]>(
    () => [
      ...spendingPeriodOptions.map(({ key, label, value }) => ({
        disabled: value === 'statement' && !hasStatements,
        disabledReason:
          value === 'statement' && !hasStatements ? 'Choose a spending card first' : undefined,
        id: `spending.period.${String(value)}`,
        title: `Show ${label} spending`,
        search: `spending period range ${label}`,
        group: 'View',
        scope: 'page' as const,
        bindings: [{ sequence: [key], label: key.toUpperCase() }],
        repeatable: true,
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
        repeatable: true,
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
        repeatable: true,
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
        repeatable: true,
        run: () => setPeriodOffset((current) => Math.min(0, current + 1)),
      },
    ],
    [
      canCompareTypical,
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
      setComparison,
      setPeriodOffset,
    ],
  )
  useRegisterActions(actions)
}
