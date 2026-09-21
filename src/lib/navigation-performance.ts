// Local performance entries contain timings only, never search parameters or finance data.
export function measureNavigation(router: {
  subscribe(
    event: 'onBeforeNavigate' | 'onRendered',
    listener: (event: { fromLocation?: unknown; pathChanged: boolean }) => void,
  ): () => void
}) {
  let started: number | undefined
  let generation = 0
  const before = router.subscribe('onBeforeNavigate', ({ fromLocation, pathChanged }) => {
    generation++
    started = fromLocation && pathChanged ? performance.now() : undefined
  })
  const rendered = router.subscribe('onRendered', () => {
    if (started === undefined) return
    const start = started
    started = undefined
    const current = generation
    requestAnimationFrame(() =>
      requestAnimationFrame(() => {
        if (current !== generation || document.hidden) return
        // Two frames approximate the first paint opportunity; this is not GPU timing.
        if (performance.getEntriesByName('brief:navigation-to-frame').length >= 40)
          performance.clearMeasures('brief:navigation-to-frame')
        performance.measure('brief:navigation-to-frame', { start, end: performance.now() })
      }),
    )
  })
  return () => {
    generation++
    before()
    rendered()
  }
}
