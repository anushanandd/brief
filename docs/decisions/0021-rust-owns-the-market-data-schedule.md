# 0021: Rust owns the market data schedule

2026-11-14 · Accepted · Extends [0006](0006-saved-summaries-refresh-in-place.md)

## Context

The page started Brief's news refreshes: the Investments warm-up asked for one, and so did the close job. Each summary build downloaded a year of daily closes again (about half a second of its 1.0–1.4 seconds, measured), duplicate requests for one summary each built it (five in six seconds, observed), a period never summarized showed a loading skeleton, and nothing paced SEC's or Alpaca's requests. Refreshes reported only that they ran, not whether anything was new, so everything downstream rebuilt on a timer.

## Decision

- One Rust job (`news_job`) refreshes stories and filings: every ten minutes from the pre-market to the end of after-hours on a trading day, hourly otherwise, and only while the window is open. Filings are checked every half hour.
- Saves say how many items were new. When any were, Rust emits `data-updated` with the holdings, the reader reads the new text, and the summaries in view are made again. While the market trades they are also made again each quarter hour, as their prices move on.
- Every SEC request takes one of ten tokens a second, and every Alpaca request one of 180 a minute with a burst of 30 (`providers::pace`).
- Settled daily closes are kept in the news cache and fetched only from a symbol's last saved session, which is fetched again to detect a split or dividend that adjusted the history.
- One summary is built at a time per holding and period; a request during a build waits for it. A period never summarized shows the holding's latest summary, marked updating, while its own is made; only a holding with nothing saved waits.

## Consequences

- No page can push a provider past its limit, and switching holdings or periods never starts a network request.
- Summary builds fetch at most a day of closes after the first.
- A summary's local work stays slower in `pnpm tauri dev` than in a release build, since the dev profile compiles every dependency unoptimized; that was left as it is.
