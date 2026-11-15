# 0024: A page's period can end in the past

2026-11-14 · Accepted

## Context

Every chart period ended today. The figures beside a chart (its change, the brokerages' statement, an account's flows, the balance changes, and the income and spending recap) are computed in Rust for each window ending today and saved with the snapshot. Scrolling a chart back in time needs those figures for a window ending on an earlier day, and the renderer may not compute them ([PRINCIPLES](../../PRINCIPLES.md): Rust computes every financial result).

## Decision

- A page's period is its range (D, W, M, Q, Y, A, shared for the session as before) and an end day, which is today unless the page is scrolled back. Only a week, month, quarter, or year scrolls. The end day resets when another page opens, and when the range changes to a day or all time.
- Rust answers `get_period_as_of(end)` (`insights::as_of`) by setting aside every history point, transaction, and trade after the end day and measuring the same windows to it, with the same functions the snapshot uses. The renderer swaps those figures in (`useAsOf`) and shows none until they arrive.
- Analytics reports are read over the period ending on the end day (Rust's report window already takes one), except custom dates and Benefits' calendar year. Spending keeps its own period offsets; scrolling its chart steps them.
- Charts move as the trackpad scrolls, and the end day moves with each whole day passed, so the figures follow while it scrolls; when it pauses, it settles on the nearest day. A chart draws its line from the saved histories, which a past day's figures arriving leave as they are, so they never interrupt a scroll. Bar charts step a whole period each time the scroll passes half their width. The holding chart, whose trading timeline is compressed, moves a day at a time, and while scrolled back it reads the security's whole daily history.
- Investment summaries are written only for periods ending today. Scrolled back, the summary card says so. The earnings and coming-up cards, which aren't the period's, keep today's summary.

## Consequences

- Each past day costs one Rust call per page, cached for the snapshot: about 9 ms in a release build and 100 ms in a debug one, measured on a year of real data. A scroll asks for one day at a time, the latest when the last answer arrives, and the nearest day answered stands in meanwhile, so the figures can trail the chart by a day while it moves.
- A past week or month on the holding chart has daily candles, not the hourly ones today's has.
- The browser preview has no native runtime, so a page scrolled back there shows its period figures as unavailable.
- Balances in the Accounts menu and the holdings stay today's. Only figures that belong to the period move with it.
