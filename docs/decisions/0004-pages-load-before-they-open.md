# 0004: Pages load before they open

2026-11-11 · Accepted

## Context

Pages read their data after they mounted. The router switched pages at once, so a page whose data wasn't cached replaced itself with a loading skeleton, as moving from Spending to Cash flow did. Two prefetches had been added for particular paths (the period Money opens on, and a strip's neighbors), each building its request apart from the page and helping only on its own path.

## Decision

A route's loader asks for its page's queries before the page opens (`src/route-data.ts`), and the router keeps the current page on screen until they arrive. The loader builds each request with the same function the page renders from, so the two always ask for the same thing. Loaders use the blocking stale-reload mode and no preload staleness, so entering a route always waits for current data; a cached query returns at once. A failed read is left to the page to show.

## Consequences

- No navigation passes through a loading screen: links, shortcuts, the strip, and history all go through the router. Hovering a link or a strip's next card runs the loader ahead.
- A change within a page, such as its range, waits for its data, so the control and the figures change together.
- A page still shows its loading state for the first snapshot, or after a second of waiting.
- The ad hoc prefetches were removed.
