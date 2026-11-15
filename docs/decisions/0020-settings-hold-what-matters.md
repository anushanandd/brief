# 0020: Settings hold what changes results or what leaves the Mac

2026-11-13 · Accepted

## Context

Settings had grown sections for tastes Brief could decide itself: how often quotes redraw (also offered in the header's status popover), a default chart range, whether Overview's account categories start collapsed, and a file to carry those preferences to another Mac. A status strip at the top repeated the header's saved time and Data health. Each was one more thing to read past to find the settings that matter.

## Decision

A setting stays when it changes a result (accounts, taxes, card rules), decides what leaves the Mac or what costs money (each connection, logos, card-term lookups, the card catalog, billed Plaid products, the local model), or is the user's own arrangement (page order and number keys). Everything else is decided by Brief:

- Quotes redraw every 10 seconds.
- The chart range is the one last chosen, remembered across launches.
- Overview's account categories switch from the palette on Overview, where they show.
- There is no preferences file; the financial backup carries everything that matters.

## Consequences

- Settings is Connections, Accounts, Taxes, Navigation, Privacy, Backup & data, and Advanced.
- A new display preference belongs where its effect shows, as a page control or palette action, not in Settings.
- A preferences file saved by an older build can no longer be imported; nothing in one changed a result.
