# 0012: Places are looked up only on request

Recorded 2026-11-11 from the earlier architecture docs; decided before then, date not recorded · Accepted

## Context

Naming the business behind an in-person purchase needs a places database (Overture), which means going online. A lookup that sent merchant names would tell an outside service where the user shops.

## Decision

Brief goes online for places only when the user asks. Only an area's coordinates leave the Mac, never a merchant name or an amount, and what comes back is cached (`places::cache`). City names and coordinates for descriptors are bundled and read offline.

## Consequences

Places work offline from the cache, and a purchase in an area never looked up has only its city.
