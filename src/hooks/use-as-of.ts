import { keepPreviousData, useQuery } from '@tanstack/react-query'
import { useEffect, useMemo, useState } from 'react'

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
  useEffect(() => {
    if (asked !== end && !query.isFetching) setAsked(end)
  }, [asked, end, query.isFetching])
  return useMemo(() => withPeriodAsOf(data, end, query.data), [data, end, query.data])
}

/** The snapshot with `past`'s figures while it is scrolled back to `end`; none known yet without. */
export function withPeriodAsOf<T extends FinanceSnapshot | undefined>(
  data: T,
  end: string | undefined,
  past: PeriodAsOf | null | undefined,
): T {
  if (!data?.insights || !end) return data
  const { end: _, ...figures } = past ?? { end, ...unknown }
  return { ...data, insights: { ...data.insights, ...figures } }
}
