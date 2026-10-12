import { useMemo } from 'react'

import { useRegisterActions, type AppAction } from '../lib/action-system'
import { type SpendingPeriod, spendingPeriodOptions } from '../lib/spending'

/** Spending's page commands: a period per key, stepping through periods, and clearing focus. */
export function useSpendingActions({
  hasStatements,
  focus,
  period,
  periodOffset,
  choosePeriod,
  setPeriodOffset,
  clearFocus,
}: {
  hasStatements: boolean
  focus: string | null
  period: SpendingPeriod
  periodOffset: number
  /** Shows a period from its latest instance. */
  choosePeriod: (period: SpendingPeriod) => void
  setPeriodOffset: (update: (current: number) => number) => void
  clearFocus: () => void
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
        id: 'spending.period.previous',
        title: 'Previous spending period',
        group: 'View',
        scope: 'page',
        bindings: [
          { sequence: ['ArrowLeft'], label: '←' },
          { sequence: ['['], label: '[' },
        ],
        disabled: period === 0,
        disabledReason: period === 0 ? 'All time has no earlier period' : undefined,
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
    [choosePeriod, clearFocus, focus, hasStatements, period, periodOffset, setPeriodOffset],
  )
  useRegisterActions(actions)
}
