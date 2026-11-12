# Design

Brief should feel like a private instrument: calm, exact, and quick. [PRINCIPLES.md](PRINCIPLES.md) says what matters; this says how it looks and moves. Tokens in `src/styles/tokens.css` are the exact values, and tests enforce the scale and contrast.

## Feel

- **Quiet chrome, earned emphasis.** Navigation recedes and the figure you came for leads. As Linear puts it, don't compete for attention you haven't earned.
- **Each region answers one question.** A page leads with its answer; supporting detail follows.
- **Evidence sits beside the value.** Saved, estimated, incomplete, and unavailable are labeled where the value appears, never by color alone.

## Layout

- Content fills the space it's given. Columns align and end together; a card no longer ends wherever its content happens to.
- Desktop only, 860 px and up. Navigation is the sidebar and cards, never top tabs.
- One period control governs a page.

## Prominence

Everything stored is reachable, in three tiers:

1. **On screen:** what you came for.
2. **One click:** everything that explains or enriches it.
3. **Deliberate:** identifiers, raw provider data, and internal versions.

## Motion

- Under 300 ms, eased out, transform and opacity only, and interruptible.
- Data arriving and keys pressed repeatedly don't animate; values update in place.
- What opens may slide in. What closes leaves at once, so nothing waits on a fade.
- Reduced motion keeps short fades and drops everything that moves.

## Keyboard

Every command is reachable from the palette (⌘K), the menu bar, and a shortcut. Arrows mean the same thing everywhere. Focus draws no ring, and on the page Tab doesn't move focus; inside a dialog, Settings included, Tab walks its controls, and a list in it (each setting is a row) steps with J and K. App shortcuts (⌘K, ⌘Z, ⌘H, navigation) still work over a dialog; page shortcuts wait for it to close. A dialog's own keys (the inspector's J, K, C, and the rest) are registered actions scoped to that dialog, so the key guide, ⌘/, and the palette list them like any other. Whether to show rings when navigating by keyboard is an open decision.

⌘1–⌘9 open the sidebar's pages in the order shown, and the menu bar carries them. Holding ⌘ alone for a moment shows every visible control's ⌘ shortcut beside it, ⌘1–⌘9 down the sidebar; the hints float over the page, so nothing moves, and vanish when ⌘ is released, another key is pressed, or the window loses focus. Nothing shows while a dialog or the palette is open.

The plain number keys 1–9 and 0 are the user's own bookmarks: each does nothing until assigned in Settings › Navigation › Number keys to a page, a report, an account, a type of account, or a held security, and then opens it from anywhere except a text field. An assigned key shows as the shortcut on its destination's palette row and in the key guide, never in the sidebar. A bookmark whose destination is gone, such as a closed account or a sold security, does nothing, and Settings names it as no longer available. A page's own single keys never use digits; a digit after a prefix (`b1` opens the first brokerage) belongs to that sequence.

## Color and type

Green and red appear only beside a signed value. Data colors identify series, never judge them. Financial values use tabular figures and come from `src/lib/format.ts`, whose formatters mask every personal amount while screen privacy (⌘H) is on; public market prices use `formatPublicCurrency`. `routes-privacy.test.tsx` fails if any page shows an amount while values are hidden. Dates and times come from there too: a calendar day reads the same in every time zone, an instant reads in the user's, and a market session time is Eastern and says so.

## References

[Linear's 2026 refresh](https://linear.app/now/behind-the-latest-design-refresh), [Emil Kowalski on animation](https://emilkowal.ski/ui/great-animations), Superlogical, and Shopify's 2026 admin.
