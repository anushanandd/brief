/** A step through the sidebar or a selector should paint within this, as speed is the feature. */
export const navigationBudgetMs = 100
const measureName = 'brief:navigation-to-frame'

// Local performance entries contain timings only, never search parameters or finance data.
export function measureNavigation(router: {
  subscribe(
    event: 'onBeforeNavigate' | 'onRendered',
    listener: (event: {
      fromLocation?: unknown
      pathChanged: boolean
      hrefChanged?: boolean
    }) => void,
  ): () => void
}) {
  let started: number | undefined
  let generation = 0
  const before = router.subscribe(
    'onBeforeNavigate',
    ({ fromLocation, pathChanged, hrefChanged }) => {
      generation++
      // Selector steps change only the search (an account, a report), so they count too.
      started = fromLocation && (pathChanged || hrefChanged) ? performance.now() : undefined
    },
  )
  const rendered = router.subscribe('onRendered', () => {
    if (started === undefined) return
    const start = started
    started = undefined
    const current = generation
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (current !== generation || document.hidden) return
        // Two frames approximate the first paint opportunity; this is not GPU timing.
        if (performance.getEntriesByName(measureName).length >= 40)
          performance.clearMeasures(measureName)
        const measure = performance.measure(measureName, { start, end: performance.now() })
        if (import.meta.env.DEV && measure.duration > navigationBudgetMs)
          console.warn(
            `[Brief] Navigation took ${Math.round(measure.duration)} ms, over the ${navigationBudgetMs} ms budget`,
          )
      }),
    )
  })
  return () => {
    generation++
    before()
    rendered()
  }
}

/** How recent navigation compares with the budget: the slowest typical step and the misses. */
export function navigationBudget() {
  const durations = performance
    .getEntriesByName(measureName)
    .map(({ duration }) => duration)
    .toSorted((left, right) => left - right)
  return {
    budgetMs: navigationBudgetMs,
    count: durations.length,
    p95Ms: durations.length
      ? Math.round(durations[Math.min(durations.length - 1, Math.floor(durations.length * 0.95))])
      : null,
    overBudget: durations.filter((duration) => duration > navigationBudgetMs).length,
  }
}
