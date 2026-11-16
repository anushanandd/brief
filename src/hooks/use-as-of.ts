import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useMemo, useState } from 'react'

import { getPeriodAsOf } from '../lib/api'
import type { PeriodAsOf } from '../lib/bindings/PeriodAsOf'
import { useGraphEnd } from '../lib/graph-preferences'
import type { FinanceSnapshot } from '../lib/schema'
import { snapshotKey } from './use-finance'

type PeriodFigures = Omit<PeriodAsOf, 'end'>

// Until a past day's figures first arrive, none of them is known.
const unknown: PeriodFigures = { chartChanges: [], periods: [], flows: [], changes: [], recap: [] }

/**
 * The snapshot with its chart-period figures as they stood at the page's end day, while the
 * page is scrolled back in time: each chart's change, the brokerages' statements, the flows,
 * the balance changes, and the recap, as Rust measured them to that day. While a scroll moves
 * the end day, Rust is asked for one day at a time, the latest when the last answer arrives,
 * and the nearest day answered so far stands in meanwhile.
 */
export function useAsOf<T extends FinanceSnapshot | undefined>(data: T): T {
  const [end] = useGraphEnd()
  const [asked, setAsked] = useState(end)
  const query = useQuery({
    queryKey: ['period-as-of', ...snapshotKey(data), asked],
    queryFn: () => getPeriodAsOf(asked!),
    enabled: Boolean(data?.insights && asked),
    staleTime: Infinity,
    retry: false,
    placeholderData: keepPreviousData,
  })
  // Set while rendering, so a new end day asks without a commit of its own.
  if (asked !== end && !query.isFetching) setAsked(end)
  return useMemo(() => withPeriodAsOf(data, end, query.data), [data, end, query.data])
}

// Snapshots standing in with another day's figures, or none, until the end day's arrive.
const waiting = new WeakSet<object>()

/** Whether `useAsOf`'s snapshot is still waiting on its end day's figures from Rust. */
export const asOfWaiting = (data: FinanceSnapshot | undefined) => Boolean(data && waiting.has(data))

/** The snapshot with `past`'s figures while it is scrolled back to `end`; none known yet without. */
export function withPeriodAsOf<T extends FinanceSnapshot | undefined>(
  data: T,
  end: string | undefined,
  past: PeriodAsOf | null | undefined,
): T {
  if (!data?.insights || !end) return data
  const { end: _, ...figures } = past ?? { end, ...unknown }
  const result = { ...data, insights: { ...data.insights, ...figures } }
  // Rust answers null for a day it has no figures for.
  if (past !== null && past?.end !== end) waiting.add(result)
  return result
}
